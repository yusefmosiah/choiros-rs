//! User preference API endpoints.
//!
//! Theme preference is user-global and persisted as EventStore events.

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::actors::event_store::{get_events_for_actor, EventStoreMsg};
use crate::api::ApiState;

const DEFAULT_THEME: &str = "dark";

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
