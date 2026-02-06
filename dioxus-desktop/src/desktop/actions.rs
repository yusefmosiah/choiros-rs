use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use shared_types::{AppDefinition, DesktopState};

use crate::api::{
    close_window, focus_window, maximize_window, minimize_window, move_window, open_window,
    resize_window, restore_window, send_chat_message,
};
use crate::desktop::state::{
    find_chat_window_id, focus_window_and_raise_z, push_window_and_activate,
    remove_window_and_reselect_active,
};

fn error_indicates_missing_window(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("not found")
        || (message.contains("window") && message.contains("no"))
        || (message.contains("window") && message.contains("missing"))
        || (message.contains("http error: 400") && message.contains("window"))
}

fn error_indicates_minimized_window(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("cannot focus minimized window")
        || (message.contains("minimized") && message.contains("focus"))
}

fn viewer_props_for_app(app_id: &str) -> Option<serde_json::Value> {
    match app_id {
        "writer" => Some(serde_json::json!({
            "viewer": {
                "kind": "text",
                "resource": {
                    "uri": "file:///workspace/README.md",
                    "mime": "text/markdown"
                },
                "capabilities": { "readonly": false }
            }
        })),
        "files" => Some(serde_json::json!({
            "viewer": {
                "kind": "image",
                "resource": {
                    "uri": "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSI3MjAiIGhlaWdodD0iNDAwIj48cmVjdCB3aWR0aD0iNzIwIiBoZWlnaHQ9IjQwMCIgZmlsbD0iIzBkMTcyYSIvPjx0ZXh0IHg9IjUwJSIgeT0iNTAlIiBmaWxsPSIjZTVlN2ViIiBmb250LXNpemU9IjMyIiBmb250LWZhbWlseT0ibW9ub3NwYWNlIiB0ZXh0LWFuY2hvcj0ibWlkZGxlIj5DaG9pciBWaWV3ZXIgTVZQPC90ZXh0Pjwvc3ZnPg==",
                    "mime": "image/svg+xml"
                },
                "capabilities": { "readonly": true }
            }
        })),
        _ => None,
    }
}

pub async fn open_app_window(
    desktop_id: String,
    app: AppDefinition,
    mut desktop_state: Signal<Option<DesktopState>>,
) {
    match open_window(
        &desktop_id,
        &app.id,
        &app.name,
        viewer_props_for_app(&app.id),
    )
    .await
    {
        Ok(window) => {
            if let Some(state) = desktop_state.write().as_mut() {
                push_window_and_activate(state, window);
            }
        }
        Err(e) => {
            dioxus_logger::tracing::error!("Failed to open window: {}", e);
        }
    }
}

pub async fn close_window_action(
    desktop_id: String,
    window_id: String,
    mut desktop_state: Signal<Option<DesktopState>>,
) {
    match close_window(&desktop_id, &window_id).await {
        Ok(_) => {
            if let Some(state) = desktop_state.write().as_mut() {
                remove_window_and_reselect_active(state, &window_id);
            }
        }
        Err(e) => {
            if error_indicates_missing_window(&e)
                || e.to_ascii_lowercase().contains("http error: 400")
            {
                if let Some(state) = desktop_state.write().as_mut() {
                    remove_window_and_reselect_active(state, &window_id);
                }
            }
            dioxus_logger::tracing::error!("Failed to close window: {}", e);
        }
    }
}

pub async fn focus_window_action(
    desktop_id: String,
    window_id: String,
    mut desktop_state: Signal<Option<DesktopState>>,
) {
    match focus_window(&desktop_id, &window_id).await {
        Ok(_) => {
            if let Some(state) = desktop_state.write().as_mut() {
                focus_window_and_raise_z(state, &window_id);
            }
        }
        Err(e) => {
            if error_indicates_minimized_window(&e) {
                if restore_window(&desktop_id, &window_id).await.is_ok() {
                    if focus_window(&desktop_id, &window_id).await.is_ok() {
                        if let Some(state) = desktop_state.write().as_mut() {
                            focus_window_and_raise_z(state, &window_id);
                        }
                        return;
                    }
                }
            }

            if error_indicates_missing_window(&e) {
                if let Some(state) = desktop_state.write().as_mut() {
                    remove_window_and_reselect_active(state, &window_id);
                }
            }
            dioxus_logger::tracing::error!("Failed to focus window: {}", e);
        }
    }
}

pub async fn move_window_action(desktop_id: String, window_id: String, x: i32, y: i32) {
    if let Err(e) = move_window(&desktop_id, &window_id, x, y).await {
        dioxus_logger::tracing::error!("Failed to move window: {}", e);
    }
}

pub async fn resize_window_action(desktop_id: String, window_id: String, width: i32, height: i32) {
    if let Err(e) = resize_window(&desktop_id, &window_id, width, height).await {
        dioxus_logger::tracing::error!("Failed to resize window: {}", e);
    }
}

pub async fn minimize_window_action(desktop_id: String, window_id: String) {
    if let Err(e) = minimize_window(&desktop_id, &window_id).await {
        dioxus_logger::tracing::error!("Failed to minimize window: {}", e);
    }
}

pub async fn maximize_window_action(desktop_id: String, window_id: String) {
    if let Err(e) = maximize_window(&desktop_id, &window_id).await {
        dioxus_logger::tracing::error!("Failed to maximize window: {}", e);
    }
}

pub async fn restore_window_action(desktop_id: String, window_id: String) {
    if let Err(e) = restore_window(&desktop_id, &window_id).await {
        dioxus_logger::tracing::error!("Failed to restore window: {}", e);
    }
}

pub async fn handle_prompt_submit(
    desktop_id: String,
    text: String,
    mut desktop_state: Signal<Option<DesktopState>>,
) {
    let chat_window_id = find_chat_window_id(&desktop_state.read());

    if let Some(window_id) = chat_window_id {
        let _ = focus_window(&desktop_id, &window_id).await;
        let _ = send_chat_message(&window_id, "user-1", &text).await;
        return;
    }

    match open_window(&desktop_id, "chat", "Chat", None).await {
        Ok(window) => {
            let window_id = window.id.clone();
            if let Some(state) = desktop_state.write().as_mut() {
                push_window_and_activate(state, window);
            }
            let _ = send_chat_message(&window_id, "user-1", &text).await;
        }
        Err(e) => {
            dioxus_logger::tracing::error!("Failed to open chat window: {}", e);
        }
    }
}
