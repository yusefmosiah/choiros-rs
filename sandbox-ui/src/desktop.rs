//! Desktop UI Components
//!
//! Mobile-first window system with taskbar/app switcher.
//! Supports both mobile (single window) and desktop (multi-window) modes.

use dioxus::prelude::*;
use shared_types::{WindowState, AppDefinition, DesktopState};

use crate::api::{
    fetch_desktop_state, open_window, close_window, focus_window,
};
use crate::components::ChatView;

/// Desktop component - manages windows and app switching
#[component]
pub fn Desktop(desktop_id: String) -> Element {
    let mut desktop_state = use_signal(|| None::<DesktopState>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let desktop_id_signal = use_signal(|| desktop_id.clone());
    
    // Load desktop state on mount
    use_effect(move || {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
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
        });
    });
    
    // Callback to open a new window
    let open_app_window = use_callback(move |app: AppDefinition| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            match open_window(&desktop_id, &app.id, &app.name, None).await {
                Ok(_window) => {
                    // Refresh desktop state
                    match fetch_desktop_state(&desktop_id).await {
                        Ok(state) => desktop_state.set(Some(state)),
                        Err(e) => dioxus_logger::tracing::error!("Failed to refresh: {}", e),
                    }
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to open window: {}", e);
                }
            }
        });
    });
    
    // Callback to close a window
    let close_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            match close_window(&desktop_id, &window_id).await {
                Ok(_) => {
                    // Refresh desktop state
                    match fetch_desktop_state(&desktop_id).await {
                        Ok(state) => desktop_state.set(Some(state)),
                        Err(e) => dioxus_logger::tracing::error!("Failed to refresh: {}", e),
                    }
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to close window: {}", e);
                }
            }
        });
    });
    
    // Callback to focus/switch to a window
    let focus_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            match focus_window(&desktop_id, &window_id).await {
                Ok(_) => {
                    // Refresh to get updated z-index
                    match fetch_desktop_state(&desktop_id).await {
                        Ok(state) => desktop_state.set(Some(state)),
                        Err(e) => dioxus_logger::tracing::error!("Failed to refresh: {}", e),
                    }
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to focus window: {}", e);
                }
            }
        });
    });
    
    let current_state = desktop_state.read();
    
    rsx! {
        div {
            class: "desktop-container",
            style: "min-height: 100vh; display: flex; flex-direction: column; background-color: #111827; color: white; overflow: hidden;",
            
            // Main content area - shows windows
            div {
                class: "desktop-content",
                style: "flex: 1; position: relative; overflow: hidden;",
                
                if loading() {
                    div {
                        style: "display: flex; align-items: center; justify-content: center; height: 100%; color: #9ca3af;",
                        "Loading desktop..."
                    }
                } else if let Some(err) = error.read().as_ref() {
                    div {
                        style: "display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: #ef4444; padding: 1rem;",
                        p { "Error loading desktop:" }
                        p { "{err}" }
                        button {
                            style: "margin-top: 1rem; padding: 0.5rem 1rem; background-color: #3b82f6; color: white; border: none; border-radius: 0.375rem; cursor: pointer;",
                            onclick: move |_| {
                                // Retry loading
                                let desktop_id = desktop_id_signal.to_string();
                                spawn(async move {
                                    loading.set(true);
                                    match fetch_desktop_state(&desktop_id).await {
                                        Ok(state) => {
                                            desktop_state.set(Some(state));
                                            error.set(None);
                                        }
                                        Err(e) => error.set(Some(e)),
                                    }
                                    loading.set(false);
                                });
                            },
                            "Retry"
                        }
                    }
                } else if let Some(state) = current_state.as_ref() {
                    // Render windows
                    if state.windows.is_empty() {
                        div {
                            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: #6b7280;",
                            p { "No windows open" }
                            p { style: "font-size: 0.875rem; margin-top: 0.5rem;", "Tap an app in the taskbar to get started" }
                        }
                    } else {
                        // Mobile: Show only the active window (or first if none active)
                        // Desktop: Show all windows with positioning
                        for window in state.windows.iter() {
                            WindowChrome {
                                window: window.clone(),
                                is_active: state.active_window.as_ref() == Some(&window.id),
                                on_close: close_window_cb.clone(),
                                on_focus: focus_window_cb.clone(),
                            }
                        }
                    }
                }
            }
            
            // Taskbar at bottom
            if let Some(state) = current_state.as_ref() {
                Taskbar {
                    apps: state.apps.clone(),
                    windows: state.windows.clone(),
                    active_window: state.active_window.clone(),
                    on_open_app: open_app_window.clone(),
                    on_switch_window: focus_window_cb.clone(),
                }
            }
        }
    }
}

/// Window chrome - wraps app content with title bar and controls
#[component]
fn WindowChrome(
    window: WindowState,
    is_active: bool,
    on_close: Callback<String>,
    on_focus: Callback<String>,
) -> Element {
    let window_id = window.id.clone();
    let window_id_for_close = window_id.clone();
    
    rsx! {
        div {
            class: if is_active { "window-chrome active" } else { "window-chrome" },
            style: "position: absolute; display: flex; flex-direction: column; background-color: #1f2937; border: 1px solid #374151; border-radius: 0.5rem; overflow: hidden; box-shadow: 0 4px 6px -1px rgba(0, 0, 0, 0.5);",
            // Mobile: full screen, Desktop: positioned
            style: if is_active { "inset: 0; margin: 0.5rem;" } else { "display: none;" },
            onclick: move |_| on_focus.call(window_id.clone()),
            
            // Title bar
            div {
                class: "window-titlebar",
                style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem; background-color: #111827; border-bottom: 1px solid #374151; cursor: default; user-select: none;",
                
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    span { "{window.title}" }
                }
                
                button {
                    class: "window-close-btn",
                    style: "padding: 0.25rem 0.5rem; background-color: transparent; color: #9ca3af; border: none; border-radius: 0.25rem; cursor: pointer; font-size: 1.25rem; line-height: 1;",
                    onclick: move |_| on_close.call(window_id_for_close.clone()),
                    "×"
                }
            }
            
            // Window content
            div {
                class: "window-content",
                style: "flex: 1; overflow: auto; padding: 0.5rem;",
                
                // Render app content based on app_id
                match window.app_id.as_str() {
                    "chat" => rsx! {
                        ChatView { actor_id: window.id.clone() }
                    },
                    _ => rsx! {
                        div {
                            style: "display: flex; align-items: center; justify-content: center; height: 100%; color: #6b7280;",
                            "App '{window.app_id}' not yet implemented"
                        }
                    }
                }
            }
        }
    }
}

/// Taskbar - shows app icons and active windows (mobile: bottom sheet style)
#[component]
fn Taskbar(
    apps: Vec<AppDefinition>,
    windows: Vec<WindowState>,
    active_window: Option<String>,
    on_open_app: Callback<AppDefinition>,
    on_switch_window: Callback<String>,
) -> Element {
    // Clone data for use in the UI
    let apps_for_windows = apps.clone();
    let has_windows = !windows.is_empty();
    
    rsx! {
        div {
            class: "taskbar",
            style: "background-color: #111827; border-top: 1px solid #374151; padding: 0.5rem;",
            
            // Active windows strip (shows open windows)
            if has_windows {
                div {
                    style: "display: flex; gap: 0.5rem; margin-bottom: 0.5rem; padding-bottom: 0.5rem; border-bottom: 1px solid #374151; overflow-x: auto;",
                    
                    for window in windows.clone().into_iter() {
                        button {
                            class: if active_window.as_ref() == Some(&window.id) { "taskbar-window-btn active" } else { "taskbar-window-btn" },
                            style: if active_window.as_ref() == Some(&window.id) { 
                                "display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem 0.75rem; background-color: #3b82f6; color: white; border: 1px solid #3b82f6; border-radius: 0.375rem; cursor: pointer; white-space: nowrap;" 
                            } else { 
                                "display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem 0.75rem; background-color: #1f2937; color: white; border: 1px solid #374151; border-radius: 0.375rem; cursor: pointer; white-space: nowrap;" 
                            },
                            onclick: move |_| on_switch_window.call(window.id.clone()),
                            
                            span { "{get_app_icon(&apps_for_windows, &window.app_id)}" }
                            span { style: "font-size: 0.875rem;", "{window.title}" }
                        }
                    }
                }
            }
            
            // App icons row
            div {
                style: "display: flex; justify-content: center; gap: 1rem;",
                
                for app in apps.clone().into_iter() {
                    button {
                        class: "taskbar-app-btn",
                        style: "display: flex; flex-direction: column; align-items: center; gap: 0.25rem; padding: 0.5rem; background-color: transparent; border: none; cursor: pointer; color: #9ca3af;",
                        onclick: move |_| on_open_app.call(app.clone()),
                        
                        span { style: "font-size: 1.5rem;", "{app.icon}" }
                        span { style: "font-size: 0.75rem;", "{app.name}" }
                    }
                }
            }
        }
    }
}

/// Helper function to get app icon by app_id
fn get_app_icon(apps: &[AppDefinition], app_id: &str) -> String {
    apps.iter()
        .find(|a| a.id == app_id)
        .map(|a| a.icon.clone())
        .unwrap_or_else(|| "📱".to_string())
}
