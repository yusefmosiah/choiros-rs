use dioxus::prelude::{Signal, WritableExt};
use shared_types::{AppDefinition, DesktopState};

use crate::api::{
    fetch_desktop_state, fetch_user_theme_preference, register_app, update_user_theme_preference,
};
use crate::desktop::state::apply_ws_event;
use crate::desktop::theme::{
    apply_theme_to_document, get_cached_theme_preference, set_cached_theme_preference,
    DEFAULT_THEME,
};
use crate::desktop::ws::connect_websocket;

pub async fn track_viewport(mut viewport: Signal<(u32, u32)>) {
    if let Ok((w, h)) = get_viewport_size().await {
        viewport.set((w, h));
    }
}

pub async fn initialize_theme(
    mut theme_initialized: Signal<bool>,
    mut current_theme: Signal<String>,
) {
    if theme_initialized() {
        return;
    }

    theme_initialized.set(true);

    if let Some(theme) = get_cached_theme_preference() {
        apply_theme_to_document(&theme);
        current_theme.set(theme);
    }

    let user_id = "user-1";
    match fetch_user_theme_preference(user_id).await {
        Ok(theme) => {
            set_cached_theme_preference(&theme);
            apply_theme_to_document(&theme);
            current_theme.set(theme);
        }
        Err(e) => {
            dioxus_logger::tracing::warn!(
                "Failed to fetch backend theme preference, using cache/default: {}",
                e
            );
            if get_cached_theme_preference().is_none() {
                apply_theme_to_document(DEFAULT_THEME);
                current_theme.set(DEFAULT_THEME.to_string());
            }
        }
    }
}

pub async fn persist_theme(next_theme: String) {
    if let Err(e) = update_user_theme_preference("user-1", &next_theme).await {
        dioxus_logger::tracing::warn!("Failed to persist theme preference: {}", e);
    }
}

pub async fn load_initial_desktop_state(
    desktop_id: String,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut desktop_state: Signal<Option<DesktopState>>,
) {
    loading.set(true);

    match fetch_desktop_state(&desktop_id).await {
        Ok(state) => {
            desktop_state.set(Some(state));
            error.set(None);
        }
        Err(e) => {
            error.set(Some(e));
        }
    }

    loading.set(false);
}

pub async fn bootstrap_websocket(
    desktop_id: String,
    mut desktop_state: Signal<Option<DesktopState>>,
    mut ws_connected: Signal<bool>,
) {
    connect_websocket(&desktop_id, move |event| {
        apply_ws_event(event, &mut desktop_state, &mut ws_connected);
    })
    .await;
}

pub async fn register_core_apps_once(
    desktop_id: String,
    apps: Vec<AppDefinition>,
    mut apps_registered: Signal<bool>,
) {
    if apps_registered() {
        return;
    }

    apps_registered.set(true);

    for app in apps {
        let _ = register_app(&desktop_id, &app).await;
    }
}

async fn get_viewport_size() -> Result<(u32, u32), String> {
    Ok((1920, 1080))
}
