//! Viewer API endpoints
//!
//! Canonical viewer content is backend-owned and persisted via EventStore.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::api::ApiState;

#[derive(Debug, Deserialize)]
pub struct ViewerContentQuery {
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchViewerContentRequest {
    pub uri: String,
    pub base_rev: i64,
    pub content: String,
    pub window_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ViewerContentResponse {
    pub success: bool,
    pub uri: String,
    pub mime: String,
    pub content: String,
    pub revision: shared_types::ViewerRevision,
    pub readonly: bool,
}

#[derive(Debug, Serialize)]
pub struct PatchViewerContentResponse {
    pub success: bool,
    pub revision: Option<shared_types::ViewerRevision>,
    pub error: Option<String>,
    pub latest: Option<ConflictLatest>,
}

#[derive(Debug, Serialize)]
pub struct ConflictLatest {
    pub content: String,
    pub revision: shared_types::ViewerRevision,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ViewerSavedPayload {
    uri: String,
    mime: String,
    base_rev: i64,
    new_rev: i64,
    content: String,
    content_hash: String,
    window_id: String,
    user_id: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct ViewerSnapshot {
    mime: String,
    content: String,
    revision: shared_types::ViewerRevision,
    readonly: bool,
}

pub async fn get_viewer_content(
    State(state): State<ApiState>,
    Query(query): Query<ViewerContentQuery>,
) -> impl IntoResponse {
    let event_store = state.app_state.event_store();
    let uri = query.uri;

    match get_latest_snapshot(&event_store, &uri).await {
        Ok(Some(snapshot)) => (
            StatusCode::OK,
            Json(ViewerContentResponse {
                success: true,
                uri,
                mime: snapshot.mime,
                content: snapshot.content,
                revision: snapshot.revision,
                readonly: snapshot.readonly,
            }),
        )
            .into_response(),
        Ok(None) => match load_initial_snapshot(&uri) {
            Ok(Some(snapshot)) => (
                StatusCode::OK,
                Json(ViewerContentResponse {
                    success: true,
                    uri,
                    mime: snapshot.mime,
                    content: snapshot.content,
                    revision: snapshot.revision,
                    readonly: snapshot.readonly,
                }),
            )
                .into_response(),
            Ok(None) => (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "success": false,
                    "error": "resource_not_found"
                })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "success": false,
                    "error": e
                })),
            )
                .into_response(),
        },
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Failed to load viewer content: {e}")
            })),
        )
            .into_response(),
    }
}

pub async fn patch_viewer_content(
    State(state): State<ApiState>,
    Json(req): Json<PatchViewerContentRequest>,
) -> impl IntoResponse {
    let event_store = state.app_state.event_store();
    let uri = req.uri.clone();
    let mime = infer_mime(&uri);

    if is_readonly_mime(&mime) {
        return (
            StatusCode::BAD_REQUEST,
            Json(PatchViewerContentResponse {
                success: false,
                revision: None,
                error: Some("readonly_resource".to_string()),
                latest: None,
            }),
        )
            .into_response();
    }

    let latest = match get_latest_snapshot(&event_store, &uri).await {
        Ok(snapshot) => snapshot,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(PatchViewerContentResponse {
                    success: false,
                    revision: None,
                    error: Some(format!("Failed to load revision: {e}")),
                    latest: None,
                }),
            )
                .into_response()
        }
    };

    let current_rev = latest.as_ref().map(|s| s.revision.rev).unwrap_or(0);
    let current_content = latest
        .as_ref()
        .map(|s| s.content.clone())
        .unwrap_or_default();

    if req.base_rev != current_rev {
        let latest_revision = latest
            .as_ref()
            .map(|s| s.revision.clone())
            .unwrap_or_else(|| make_revision(0));

        let payload = json!({
            "uri": uri,
            "mime": mime,
            "base_rev": req.base_rev,
            "new_rev": current_rev,
            "content_hash": hash_content(&current_content),
            "window_id": req.window_id.clone().unwrap_or_default(),
            "user_id": req.user_id.clone().unwrap_or_else(|| "user-1".to_string()),
            "updated_at": chrono::Utc::now().to_rfc3339(),
        });

        let append = AppendEvent {
            event_type: shared_types::EVENT_VIEWER_CONTENT_CONFLICT.to_string(),
            payload,
            actor_id: viewer_actor_id(&req.uri),
            user_id: req.user_id.clone().unwrap_or_else(|| "user-1".to_string()),
        };
        let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append {
            event: append,
            reply,
        });

        return (
            StatusCode::CONFLICT,
            Json(PatchViewerContentResponse {
                success: false,
                revision: None,
                error: Some("revision_conflict".to_string()),
                latest: Some(ConflictLatest {
                    content: current_content,
                    revision: latest_revision,
                }),
            }),
        )
            .into_response();
    }

    let new_rev = current_rev + 1;
    let updated_at = chrono::Utc::now().to_rfc3339();
    let payload = ViewerSavedPayload {
        uri: req.uri.clone(),
        mime: mime.clone(),
        base_rev: req.base_rev,
        new_rev,
        content: req.content.clone(),
        content_hash: hash_content(&req.content),
        window_id: req.window_id.unwrap_or_default(),
        user_id: req.user_id.clone().unwrap_or_else(|| "user-1".to_string()),
        updated_at: updated_at.clone(),
    };

    let append = AppendEvent {
        event_type: shared_types::EVENT_VIEWER_CONTENT_SAVED.to_string(),
        payload: serde_json::to_value(payload).unwrap_or_else(|_| json!({})),
        actor_id: viewer_actor_id(&req.uri),
        user_id: req.user_id.unwrap_or_else(|| "user-1".to_string()),
    };

    match ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: append,
        reply,
    }) {
        Ok(Ok(_event)) => (
            StatusCode::OK,
            Json(PatchViewerContentResponse {
                success: true,
                revision: Some(shared_types::ViewerRevision {
                    rev: new_rev,
                    updated_at,
                }),
                error: None,
                latest: None,
            }),
        )
            .into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PatchViewerContentResponse {
                success: false,
                revision: None,
                error: Some(format!("Failed to persist content: {e}")),
                latest: None,
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PatchViewerContentResponse {
                success: false,
                revision: None,
                error: Some(format!("Event store actor error: {e}")),
                latest: None,
            }),
        )
            .into_response(),
    }
}

fn viewer_actor_id(uri: &str) -> String {
    format!("viewer:{uri}")
}

fn file_path_from_uri(uri: &str) -> Option<String> {
    uri.strip_prefix("file://").map(|path| path.to_string())
}

fn infer_mime(uri: &str) -> String {
    let Some(path) = file_path_from_uri(uri) else {
        return "text/plain".to_string();
    };

    match Path::new(&path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "md" | "markdown" => "text/markdown".to_string(),
        "txt" | "rs" | "toml" | "json" | "yaml" | "yml" | "js" | "ts" | "tsx" | "css" | "html" => {
            "text/plain".to_string()
        }
        "png" => "image/png".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "gif" => "image/gif".to_string(),
        "webp" => "image/webp".to_string(),
        "svg" => "image/svg+xml".to_string(),
        _ => "text/plain".to_string(),
    }
}

fn is_readonly_mime(mime: &str) -> bool {
    mime.starts_with("image/")
}

fn hash_content(content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&content, &mut hasher);
    format!("{:x}", std::hash::Hasher::finish(&hasher))
}

fn make_revision(rev: i64) -> shared_types::ViewerRevision {
    shared_types::ViewerRevision {
        rev,
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

fn load_initial_snapshot(uri: &str) -> Result<Option<ViewerSnapshot>, String> {
    if uri.starts_with("data:image/") {
        let mime = uri
            .split(';')
            .next()
            .and_then(|part| part.strip_prefix("data:"))
            .unwrap_or("image/png")
            .to_string();
        return Ok(Some(ViewerSnapshot {
            mime,
            content: uri.to_string(),
            revision: make_revision(0),
            readonly: true,
        }));
    }

    let Some(path) = file_path_from_uri(uri) else {
        return Ok(None);
    };
    let mime = infer_mime(uri);

    if mime.starts_with("image/") {
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Failed to read image: {e}")),
        };
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        let content = format!("data:{mime};base64,{encoded}");
        return Ok(Some(ViewerSnapshot {
            mime,
            content,
            revision: make_revision(0),
            readonly: true,
        }));
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Failed to read text file: {e}")),
    };

    Ok(Some(ViewerSnapshot {
        mime,
        content,
        revision: make_revision(0),
        readonly: false,
    }))
}

async fn get_latest_snapshot(
    event_store: &ractor::ActorRef<EventStoreMsg>,
    uri: &str,
) -> Result<Option<ViewerSnapshot>, String> {
    let events = match ractor::call!(event_store, |reply| EventStoreMsg::GetEventsForActor {
        actor_id: viewer_actor_id(uri),
        since_seq: 0,
        reply,
    }) {
        Ok(Ok(events)) => events,
        Ok(Err(e)) => return Err(e.to_string()),
        Err(e) => return Err(e.to_string()),
    };

    let mut latest: Option<ViewerSnapshot> = None;
    for event in events {
        if event.event_type != shared_types::EVENT_VIEWER_CONTENT_SAVED {
            continue;
        }
        let Ok(payload) = serde_json::from_value::<ViewerSavedPayload>(event.payload.clone())
        else {
            continue;
        };

        latest = Some(ViewerSnapshot {
            mime: payload.mime.clone(),
            content: payload.content.clone(),
            revision: shared_types::ViewerRevision {
                rev: payload.new_rev,
                updated_at: payload.updated_at.clone(),
            },
            readonly: is_readonly_mime(&payload.mime),
        });
    }

    Ok(latest)
}
