use dioxus::prelude::{Signal, WritableExt};
use shared_types::{AppDefinition, DesktopState};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

use crate::api::{
    fetch_desktop_state, fetch_user_theme_preference, register_app, update_user_theme_preference,
};
use crate::desktop::theme::{
    apply_theme_to_document, get_cached_theme_preference, set_cached_theme_preference,
    DEFAULT_THEME,
};
use crate::desktop::ws::{connect_websocket, DesktopWsRuntime};

pub async fn track_viewport(mut viewport: Signal<(u32, u32)>) {
    if let Some((w, h)) = current_viewport_size() {
        viewport.set((w, h));
    }

    let Some(window) = web_sys::window() else {
        return;
    };

    let callback = Closure::wrap(Box::new(move |_event: web_sys::Event| {
        if let Some((w, h)) = current_viewport_size() {
            viewport.set((w, h));
        }
    }) as Box<dyn FnMut(web_sys::Event)>);

    let _ = window.add_event_listener_with_callback("resize", callback.as_ref().unchecked_ref());
    let _ = window
        .add_event_listener_with_callback("orientationchange", callback.as_ref().unchecked_ref());

    // Keep listener alive for app lifetime.
    callback.forget();
}

fn current_viewport_size() -> Option<(u32, u32)> {
    let window = web_sys::window()?;
    let width = window.inner_width().ok()?.as_f64()?;
    let height = window.inner_height().ok()?.as_f64()?;

    if width > 0.0 && height > 0.0 {
        return Some((width.round() as u32, height.round() as u32));
    }

    let document = window.document()?;
    let root = document.document_element()?;
    let width = root.client_width().max(0) as u32;
    let height = root.client_height().max(0) as u32;
    Some((width, height))
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

pub fn bootstrap_websocket<F>(desktop_id: String, on_event: F) -> Result<DesktopWsRuntime, String>
where
    F: FnMut(crate::desktop::ws::WsEvent) + 'static,
{
    connect_websocket(&desktop_id, on_event)
}

pub async fn register_core_apps_once(
    desktop_id: String,
    apps: Vec<AppDefinition>,
    mut apps_registered: Signal<bool>,
) {
    if apps_registered() {
        return;
    }
    const MAX_ATTEMPTS: u32 = 3;

    for attempt in 1..=MAX_ATTEMPTS {
        let mut success_count = 0usize;

        for app in &apps {
            match register_app(&desktop_id, app).await {
                Ok(()) => {
                    success_count += 1;
                }
                Err(e) => {
                    dioxus_logger::tracing::warn!(
                        "register_app failed (desktop_id={}, app_id={}, attempt={}): {}",
                        desktop_id,
                        app.id,
                        attempt,
                        e
                    );
                }
            }
        }

        if success_count > 0 {
            apps_registered.set(true);
            return;
        }

        if attempt < MAX_ATTEMPTS {
            // Allow auth/session and proxy startup to settle before retrying.
            gloo_timers::future::TimeoutFuture::new(250 * attempt).await;
        }
    }

    dioxus_logger::tracing::warn!(
        "Failed to register any core apps for desktop_id={} after {} attempts",
        desktop_id,
        MAX_ATTEMPTS
    );
}
