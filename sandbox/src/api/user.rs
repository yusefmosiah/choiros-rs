//! User preference API endpoints.
//!
//! Theme and model preferences are user-global, persisted as EventStore events.

use std::collections::HashMap;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actors::event_store::{get_events_for_actor, EventStoreMsg};
use crate::actors::model_config::ModelRegistry;
use crate::api::ApiState;

const DEFAULT_THEME: &str = "dark";
const CALLSITES: &[&str] = &[
    "conductor",
    "writer",
    "terminal",
    "researcher",
    "summarizer",
    "watcher",
];

#[derive(Debug, Deserialize)]
pub struct UpdateUserPreferencesRequest {
    pub theme: String,
}

#[derive(Debug, Serialize)]
pub struct UserPreferencesResponse {
    pub success: bool,
    pub theme: String,
}

/// Get user-global preferences.
pub async fn get_user_preferences(
    Path(user_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let event_store = state.app_state.event_store();
    let actor_id = user_actor_id(&user_id);

    match get_events_for_actor(&event_store, actor_id, 0).await {
        Ok(Ok(events)) => {
            let theme = events
                .iter()
                .rev()
                .find(|event| event.event_type == shared_types::EVENT_USER_THEME_PREFERENCE)
                .and_then(|event| event.payload.get("theme"))
                .and_then(|value| value.as_str())
                .filter(|theme| is_allowed_theme(theme))
                .unwrap_or(DEFAULT_THEME)
                .to_string();

            (
                StatusCode::OK,
                Json(UserPreferencesResponse {
                    success: true,
                    theme,
                }),
            )
                .into_response()
        }
        Ok(Err(_)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": "EventStore error"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Failed to get preferences: {}", e)
            })),
        )
            .into_response(),
    }
}

/// Update user-global preferences.
pub async fn update_user_preferences(
    Path(user_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
    Json(req): Json<UpdateUserPreferencesRequest>,
) -> impl IntoResponse {
    if !is_allowed_theme(&req.theme) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": "theme must be 'light' or 'dark'"
            })),
        )
            .into_response();
    }

    let event_store = state.app_state.event_store();
    let append_event = crate::actors::event_store::AppendEvent {
        event_type: shared_types::EVENT_USER_THEME_PREFERENCE.to_string(),
        payload: json!({ "theme": req.theme }),
        actor_id: user_actor_id(&user_id),
        user_id,
    };

    match ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: append_event,
        reply,
    }) {
        Ok(Ok(event)) => {
            let theme = event
                .payload
                .get("theme")
                .and_then(|value| value.as_str())
                .unwrap_or(DEFAULT_THEME)
                .to_string();
            (
                StatusCode::OK,
                Json(UserPreferencesResponse {
                    success: true,
                    theme,
                }),
            )
                .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": e.to_string()
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": format!("Actor error: {}", e)
            })),
        )
            .into_response(),
    }
}

fn user_actor_id(user_id: &str) -> String {
    format!("user:{user_id}")
}

fn is_allowed_theme(theme: &str) -> bool {
    matches!(theme, "light" | "dark")
}

// ── Model catalog + per-user model config ───────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ModelCatalogResponse {
    pub models: Vec<ModelInfo>,
    pub callsites: Vec<String>,
    pub defaults: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
}

/// GET /models/catalog — available models + callsites + defaults
pub async fn get_model_catalog() -> impl IntoResponse {
    let registry = ModelRegistry::new();
    let models: Vec<ModelInfo> = registry
        .available_model_ids()
        .into_iter()
        .map(|id| {
            let name = registry
                .get(&id)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| id.clone());
            ModelInfo { id, name }
        })
        .collect();

    let defaults = registry.callsite_defaults();

    Json(ModelCatalogResponse {
        models,
        callsites: CALLSITES.iter().map(|s| s.to_string()).collect(),
        defaults,
    })
}

#[derive(Debug, Deserialize)]
pub struct UpdateModelConfigRequest {
    /// Map of callsite -> model_id (e.g., {"conductor": "InceptionMercury2"})
    pub callsite_models: HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ModelConfigResponse {
    pub success: bool,
    pub callsite_models: HashMap<String, String>,
}

/// GET /user/{user_id}/model-config — current per-callsite model selections
pub async fn get_model_config(
    Path(user_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
) -> impl IntoResponse {
    let event_store = state.app_state.event_store();
    let actor_id = user_actor_id(&user_id);

    let registry = ModelRegistry::new();
    let mut callsite_models = registry.callsite_defaults();

    match get_events_for_actor(&event_store, actor_id, 0).await {
        Ok(Ok(events)) => {
            // Find most recent model selection event
            if let Some(event) = events
                .iter()
                .rev()
                .find(|e| e.event_type == shared_types::EVENT_MODEL_SELECTION)
            {
                if let Some(selections) = event.payload.get("callsite_models") {
                    if let Some(map) = selections.as_object() {
                        for (k, v) in map {
                            if let Some(model_id) = v.as_str() {
                                callsite_models.insert(k.clone(), model_id.to_string());
                            }
                        }
                    }
                }
            }
            Json(ModelConfigResponse {
                success: true,
                callsite_models,
            })
            .into_response()
        }
        _ => Json(ModelConfigResponse {
            success: true,
            callsite_models,
        })
        .into_response(),
    }
}

/// PATCH /user/{user_id}/model-config — update per-callsite model selections
pub async fn update_model_config(
    Path(user_id): Path<String>,
    axum::extract::State(state): axum::extract::State<ApiState>,
    Json(req): Json<UpdateModelConfigRequest>,
) -> impl IntoResponse {
    let registry = ModelRegistry::new();

    // Validate all callsites and model IDs
    for (callsite, model_id) in &req.callsite_models {
        if !CALLSITES.contains(&callsite.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "success": false,
                    "error": format!("unknown callsite: {callsite}")
                })),
            )
                .into_response();
        }
        if registry.get(model_id).is_none() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "success": false,
                    "error": format!("unknown model: {model_id}")
                })),
            )
                .into_response();
        }
    }

    let event_store = state.app_state.event_store();
    let append_event = crate::actors::event_store::AppendEvent {
        event_type: shared_types::EVENT_MODEL_SELECTION.to_string(),
        payload: json!({ "callsite_models": req.callsite_models }),
        actor_id: user_actor_id(&user_id),
        user_id,
    };

    match ractor::call!(event_store, |reply| EventStoreMsg::Append {
        event: append_event,
        reply,
    }) {
        Ok(Ok(event)) => {
            let callsite_models = event
                .payload
                .get("callsite_models")
                .and_then(|v| serde_json::from_value::<HashMap<String, String>>(v.clone()).ok())
                .unwrap_or_default();
            Json(ModelConfigResponse {
                success: true,
                callsite_models,
            })
            .into_response()
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": e.to_string() })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "success": false, "error": format!("Actor error: {e}") })),
        )
            .into_response(),
    }
}
