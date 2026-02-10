use dioxus::prelude::{ReadableExt, Signal, WritableExt};
use shared_types::{AppDefinition, DesktopState};

use crate::api::{
    close_window, focus_window, maximize_window, minimize_window, move_window, open_window,
    resize_window, restore_window, MaximizeWindowRequest,
};
use crate::desktop::state::{
    focus_window_and_raise_z, push_window_and_activate, remove_window_and_reselect_active,
};
use crate::interop::get_window_canvas_size;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowDesktopSnapshot {
    pub restore_window_ids: Vec<String>,
    pub previously_active_window: Option<String>,
}

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
        _ => None,
    }
}

fn maximize_work_area_request() -> Option<MaximizeWindowRequest> {
    let (width, height) = get_window_canvas_size()?;
    Some(MaximizeWindowRequest {
        x: 0,
        y: 0,
        width,
        height,
    })
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
    if let Some(state) = desktop_state.write().as_mut() {
        let can_focus_locally = state
            .windows
            .iter()
            .find(|window| window.id == window_id)
            .map(|window| !window.minimized)
            .unwrap_or(false);
        if can_focus_locally {
            focus_window_and_raise_z(state, &window_id);
        }
    }

    match focus_window(&desktop_id, &window_id).await {
        Ok(_) => {}
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

pub async fn minimize_all_windows_action(
    desktop_id: String,
    mut desktop_state: Signal<Option<DesktopState>>,
) {
    let window_ids = if let Some(state) = desktop_state.read().as_ref() {
        state
            .windows
            .iter()
            .filter(|window| !window.minimized)
            .map(|window| window.id.clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if let Some(state) = desktop_state.write().as_mut() {
        for window in &mut state.windows {
            window.minimized = true;
            window.maximized = false;
        }
        state.active_window = None;
    }

    for window_id in window_ids {
        if let Err(e) = minimize_window(&desktop_id, &window_id).await {
            dioxus_logger::tracing::error!("Failed to minimize window {window_id}: {}", e);
        }
    }
}

pub async fn toggle_show_desktop_action(
    desktop_id: String,
    desktop_state: Signal<Option<DesktopState>>,
    mut show_desktop_snapshot: Signal<Option<ShowDesktopSnapshot>>,
) {
    let current_snapshot = show_desktop_snapshot.read().as_ref().cloned();
    if let Some(snapshot) = current_snapshot {
        for window_id in snapshot.restore_window_ids.clone() {
            if let Err(e) = restore_window(&desktop_id, &window_id).await {
                dioxus_logger::tracing::error!("Failed to restore window {window_id}: {}", e);
            }
        }

        if let Some(active_window) = snapshot.previously_active_window {
            let _ = focus_window(&desktop_id, &active_window).await;
        }

        show_desktop_snapshot.set(None);
        return;
    }

    let snapshot = if let Some(state) = desktop_state.read().as_ref() {
        ShowDesktopSnapshot {
            restore_window_ids: state
                .windows
                .iter()
                .filter(|window| !window.minimized)
                .map(|window| window.id.clone())
                .collect(),
            previously_active_window: state.active_window.clone(),
        }
    } else {
        ShowDesktopSnapshot {
            restore_window_ids: Vec::new(),
            previously_active_window: None,
        }
    };

    if snapshot.restore_window_ids.is_empty() {
        return;
    }

    show_desktop_snapshot.set(Some(snapshot.clone()));
    minimize_all_windows_action(desktop_id, desktop_state).await;
}

pub async fn maximize_window_action(
    desktop_id: String,
    window_id: String,
    mut desktop_state: Signal<Option<DesktopState>>,
) {
    let work_area = maximize_work_area_request();

    if let Some(state) = desktop_state.write().as_mut() {
        let next_z = state.windows.iter().map(|w| w.z_index).max().unwrap_or(0) + 1;
        if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
            window.minimized = false;
            window.maximized = true;
            if let Some(bounds) = work_area {
                window.x = bounds.x;
                window.y = bounds.y;
                window.width = bounds.width;
                window.height = bounds.height;
            }
            window.z_index = next_z;
        }
        state.active_window = Some(window_id.clone());
    }

    if let Err(e) = maximize_window(&desktop_id, &window_id, work_area).await {
        dioxus_logger::tracing::error!("Failed to maximize window: {}", e);
    }
}

pub async fn restore_window_action(desktop_id: String, window_id: String) {
    if let Err(e) = restore_window(&desktop_id, &window_id).await {
        dioxus_logger::tracing::error!("Failed to restore window: {}", e);
    }
}
