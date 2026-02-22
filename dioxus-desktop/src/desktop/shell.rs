use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use shared_types::DesktopState;

use crate::auth::{probe_session, AuthModal, AuthState};
use crate::desktop::actions;
use crate::desktop::actions::ShowDesktopSnapshot;
use crate::desktop::apps::core_apps;
use crate::desktop::components::prompt_bar::{PromptBar, TelemetryStreamState};
use crate::desktop::components::workspace_canvas::WorkspaceCanvas;
use crate::desktop::effects;
use crate::desktop::state::{apply_ws_event, update_writer_runs_from_event};
use crate::desktop::theme::{
    apply_theme_to_document, next_theme, set_cached_theme_preference, DEFAULT_THEME,
};
use crate::desktop::ws::{DesktopWsRuntime, WsEvent};
use crate::interop::get_viewport_size;

#[component]
pub fn DesktopShell(desktop_id: String) -> Element {
    // Auth context — provided to the whole subtree
    let mut auth = use_context_provider(|| Signal::new(AuthState::default()));
    let require_auth_from_url = use_signal(should_require_auth_from_url);

    let mut desktop_state = use_signal(|| None::<DesktopState>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut ws_connected = use_signal(|| false);
    let mut desktop_ws_runtime = use_signal(|| None::<DesktopWsRuntime>);
    let ws_event_queue = use_hook(|| Rc::new(RefCell::new(VecDeque::<WsEvent>::new())));
    let mut ws_event_pump_started = use_signal(|| false);
    let ws_event_pump_alive = use_hook(|| Rc::new(Cell::new(true)));
    let desktop_id_signal = use_signal(|| desktop_id.clone());
    let viewport = use_signal(get_viewport_size);
    let apps_registered = use_signal(|| false);
    let theme_initialized = use_signal(|| false);
    let mut current_theme = use_signal(|| DEFAULT_THEME.to_string());
    let show_desktop_snapshot = use_signal(|| None::<ShowDesktopSnapshot>);
    let mut telemetry_state = use_signal(|| TelemetryStreamState::new(10)); // Max 10 telemetry lines

    {
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_drop(move || {
            ws_event_pump_alive.set(false);
        });
    }

    use_effect(move || {
        spawn(async move {
            effects::track_viewport(viewport).await;
        });
    });

    // Probe /auth/me once on load so other components know session state.
    use_effect(move || {
        spawn(async move {
            probe_session(auth).await;
        });
    });

    // If the app boots on /login or /register, force the auth modal.
    // This keeps URL intent authoritative even after /auth/me reports unauthenticated.
    use_effect(move || {
        if !require_auth_from_url() {
            return;
        }
        let current = auth.read().clone();
        if matches!(current, AuthState::Unknown | AuthState::Unauthenticated) {
            auth.set(AuthState::Required);
        }
    });

    use_effect(move || {
        let is_mobile = viewport.read().0 <= 1024;
        let Some(window) = web_sys::window() else {
            return;
        };
        let Some(document) = window.document() else {
            return;
        };

        if let Ok(Some(meta)) = document.query_selector("meta[name='viewport']") {
            let content = if is_mobile {
                "width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover"
            } else {
                "width=device-width, initial-scale=1"
            };
            let _ = meta.set_attribute("content", content);
        }

        if let Some(root) = document.document_element() {
            let _ = root.set_attribute(
                "style",
                "height: 100%; overflow: hidden; overscroll-behavior: none;",
            );
        }

        if let Some(body) = document.body() {
            let _ = body.set_attribute(
                "style",
                "margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; overscroll-behavior: none;",
            );
            if is_mobile {
                let _ = body.set_attribute(
                    "style",
                    "margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; overscroll-behavior: none; position: fixed; inset: 0;",
                );
            } else {
                let _ = body.set_attribute(
                    "style",
                    "margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; overscroll-behavior: none;",
                );
            }
        }
    });

    use_effect(move || {
        spawn(async move {
            effects::initialize_theme(theme_initialized, current_theme).await;
        });
    });

    let toggle_theme = use_callback(move |_| {
        let next = next_theme(&current_theme());
        current_theme.set(next.clone());
        apply_theme_to_document(&next);
        set_cached_theme_preference(&next);

        spawn(async move {
            effects::persist_theme(next).await;
        });
    });

    // Desktop API currently requires an authenticated session through hypervisor middleware.
    // Avoid loading desktop state pre-auth (which returns HTML redirects and stale parse errors),
    // then load/reload once auth is established.
    use_effect(move || {
        let auth_state = auth.read().clone();
        if !matches!(auth_state, AuthState::Authenticated(_)) {
            loading.set(false);
            error.set(None);
            desktop_state.set(None);
            return;
        }

        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            effects::load_initial_desktop_state(desktop_id, loading, error, desktop_state).await;
        });
    });

    {
        let ws_event_queue = ws_event_queue.clone();
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_effect(move || {
            if ws_event_pump_started() {
                return;
            }
            ws_event_pump_started.set(true);

            let ws_event_queue = ws_event_queue.clone();
            let ws_event_pump_alive = ws_event_pump_alive.clone();
            spawn(async move {
                while ws_event_pump_alive.get() {
                    let mut drained = Vec::new();
                    {
                        let mut queue = ws_event_queue.borrow_mut();
                        while let Some(event) = queue.pop_front() {
                            drained.push(event);
                        }
                    }

                    for event in drained {
                        // Handle telemetry events separately
                        if let WsEvent::Telemetry {
                            event_type,
                            capability,
                            phase,
                            importance,
                            data,
                        } = &event
                        {
                            use shared_types::EventImportance;
                            let importance_enum = match importance.as_str() {
                                "high" => EventImportance::High,
                                "low" => EventImportance::Low,
                                _ => EventImportance::Normal,
                            };
                            let message =
                                crate::desktop::components::prompt_bar::format_telemetry_message(
                                    event_type, data,
                                );
                            telemetry_state.write().add_line(
                                message,
                                capability.clone(),
                                phase.clone(),
                                importance_enum,
                            );
                        } else if matches!(
                            event,
                            WsEvent::WriterRunStarted { .. }
                                | WsEvent::WriterRunProgress { .. }
                                | WsEvent::WriterRunPatch { .. }
                                | WsEvent::WriterRunStatus { .. }
                                | WsEvent::WriterRunFailed { .. }
                        ) {
                            update_writer_runs_from_event(&event);
                        } else {
                            apply_ws_event(event, &mut desktop_state, &mut ws_connected);
                        }
                    }

                    TimeoutFuture::new(16).await;
                }
            });
        });
    }

    {
        let ws_event_queue = ws_event_queue.clone();
        use_effect(move || {
            let auth_state = auth.read().clone();
            if !matches!(auth_state, AuthState::Authenticated(_)) {
                if desktop_ws_runtime.read().is_some() {
                    desktop_ws_runtime.set(None);
                }
                ws_connected.set(false);
                return;
            }

            let desktop_id = desktop_id_signal.read().clone();
            if desktop_ws_runtime.read().is_some() {
                return;
            }

            let ws_event_queue_for_cb = ws_event_queue.clone();
            match effects::bootstrap_websocket(desktop_id, move |event| {
                ws_event_queue_for_cb.borrow_mut().push_back(event);
            }) {
                Ok(runtime) => {
                    desktop_ws_runtime.set(Some(runtime));
                }
                Err(e) => {
                    ws_connected.set(false);
                    error.set(Some(format!("Failed to connect desktop websocket: {e}")));
                }
            }
        });
    }

    let open_app_window = use_callback(move |app| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::open_app_window(desktop_id, app, desktop_state).await;
        });
    });

    let close_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::close_window_action(desktop_id, window_id, desktop_state).await;
        });
    });

    let focus_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::focus_window_action(desktop_id, window_id, desktop_state).await;
        });
    });

    let move_window_cb = use_callback(move |(window_id, x, y): (String, i32, i32)| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::move_window_action(desktop_id, window_id, x, y).await;
        });
    });

    let resize_window_cb = use_callback(move |(window_id, width, height): (String, i32, i32)| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::resize_window_action(desktop_id, window_id, width, height).await;
        });
    });

    let minimize_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::minimize_window_action(desktop_id, window_id).await;
        });
    });

    let show_desktop_cb = use_callback(move |_| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::toggle_show_desktop_action(desktop_id, desktop_state, show_desktop_snapshot)
                .await;
        });
    });

    let maximize_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::maximize_window_action(desktop_id, window_id, desktop_state).await;
        });
    });

    let restore_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.read().clone();
        spawn(async move {
            actions::restore_window_action(desktop_id, window_id).await;
        });
    });

    let core_apps = core_apps();

    {
        let apps = core_apps.clone();
        use_effect(move || {
            let auth_state = auth.read().clone();
            if !matches!(auth_state, AuthState::Authenticated(_)) {
                return;
            }

            let desktop_id = desktop_id_signal.read().clone();
            let apps = apps.clone();
            spawn(async move {
                effects::register_core_apps_once(desktop_id, apps, apps_registered).await;
            });
        });
    }

    let state_snapshot = desktop_state.read().clone();
    let is_desktop = viewport.read().0 > 1024;
    let windows = state_snapshot
        .as_ref()
        .map(|state| state.windows.clone())
        .unwrap_or_default();
    let active_window = state_snapshot
        .as_ref()
        .and_then(|state| state.active_window.clone());

    rsx! {
        style { {DEFAULT_TOKENS} }

        link {
            rel: "stylesheet",
            href: "/xterm.css",
        }

        div {
            class: "desktop-shell",
            style: "width: 100vw; height: 100dvh; min-height: 100dvh; max-height: 100dvh; display: flex; flex-direction: column; overflow: hidden;",

            WorkspaceCanvas {
                desktop_id: desktop_id_signal.read().clone(),
                apps: core_apps,
                on_open_app: open_app_window,
                is_mobile: !is_desktop,
                loading,
                error,
                state: desktop_state,
                viewport,
                on_close: close_window_cb,
                on_focus: focus_window_cb,
                on_move: move_window_cb,
                on_resize: resize_window_cb,
                on_minimize: minimize_window_cb,
                on_maximize: maximize_window_cb,
                on_restore: restore_window_cb,
            }

            PromptBar {
                connected: ws_connected(),
                is_mobile: !is_desktop,
                windows,
                active_window,
                desktop_id: desktop_id_signal.read().clone(),
                on_focus_window: focus_window_cb,
                on_show_desktop: show_desktop_cb,
                current_theme: current_theme(),
                on_toggle_theme: toggle_theme,
                telemetry_state,
            }
        }

        // Auth modal — renders as fixed overlay when AuthState::Required
        AuthModal {}
    }
}

fn should_require_auth_from_url() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let Ok(pathname) = window.location().pathname() else {
        return false;
    };
    matches!(pathname.as_str(), "/login" | "/register")
}

const DEFAULT_TOKENS: &str = r#"
:root {
    /* Colors */
    --bg-primary: #0f172a;
    --bg-secondary: #1e293b;
    --text-primary: #f8fafc;
    --text-secondary: #94a3b8;
    --text-muted: #64748b;
    --accent-bg: #3b82f6;
    --accent-bg-hover: #2563eb;
    --accent-text: #ffffff;
    --border-color: #334155;

    /* Semantic colors */
    --window-bg: var(--bg-secondary);
    --titlebar-bg: var(--bg-primary);
    --dock-bg: rgba(30, 41, 59, 0.8);
    --promptbar-bg: var(--bg-primary);
    --input-bg: var(--bg-secondary);
    --hover-bg: rgba(255, 255, 255, 0.1);
    --danger-bg: #ef4444;
    --danger-text: #ef4444;
    --success-bg: #10b981;
    --warning-bg: #f59e0b;

    /* Chat-specific colors */
    --chat-bg: var(--bg-primary);
    --chat-header-bg: var(--bg-secondary);
    --user-bubble-bg: var(--accent-bg);
    --assistant-bubble-bg: var(--bg-secondary);

    /* Spacing & Radius */
    --radius-sm: 4px;
    --radius-md: 8px;
    --radius-lg: 12px;

    /* Shadows */
    --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
    --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.4);
    --shadow-lg: 0 10px 40px rgba(0, 0, 0, 0.5);
}

:root[data-theme="dark"] {
    --bg-primary: #0f172a;
    --bg-secondary: #1e293b;
    --text-primary: #f8fafc;
    --text-secondary: #94a3b8;
    --text-muted: #64748b;
    --accent-bg: #3b82f6;
    --accent-bg-hover: #2563eb;
    --accent-text: #ffffff;
    --border-color: #334155;
    --window-bg: var(--bg-secondary);
    --titlebar-bg: var(--bg-primary);
    --dock-bg: rgba(30, 41, 59, 0.8);
    --promptbar-bg: var(--bg-primary);
    --input-bg: var(--bg-secondary);
    --hover-bg: rgba(255, 255, 255, 0.1);
    --danger-bg: #ef4444;
    --danger-text: #ef4444;
    --success-bg: #10b981;
    --warning-bg: #f59e0b;
    --chat-bg: var(--bg-primary);
    --chat-header-bg: var(--bg-secondary);
    --user-bubble-bg: var(--accent-bg);
    --assistant-bubble-bg: var(--bg-secondary);
}

:root[data-theme="light"] {
    --bg-primary: #f8fafc;
    --bg-secondary: #ffffff;
    --text-primary: #0f172a;
    --text-secondary: #475569;
    --text-muted: #64748b;
    --accent-bg: #2563eb;
    --accent-bg-hover: #1d4ed8;
    --accent-text: #ffffff;
    --border-color: #cbd5e1;
    --window-bg: var(--bg-secondary);
    --titlebar-bg: #e2e8f0;
    --dock-bg: rgba(255, 255, 255, 0.9);
    --promptbar-bg: #e2e8f0;
    --input-bg: #ffffff;
    --hover-bg: rgba(15, 23, 42, 0.08);
    --danger-bg: #dc2626;
    --danger-text: #b91c1c;
    --success-bg: #059669;
    --warning-bg: #d97706;
    --chat-bg: #f1f5f9;
    --chat-header-bg: #e2e8f0;
    --user-bubble-bg: var(--accent-bg);
    --assistant-bubble-bg: #ffffff;
}

* {
    box-sizing: border-box;
}

html, body, #main {
    width: 100%;
    height: 100%;
    overflow: hidden;
    overscroll-behavior: none;
}

body {
    margin: 0;
    padding: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
}

.desktop-icon:hover {
    background: var(--hover-bg, rgba(255, 255, 255, 0.1));
}

.desktop-icon:hover div {
    background: var(--window-bg, #1f2937);
    transform: scale(1.05);
}

.running-app:hover {
    background: var(--hover-bg, rgba(255, 255, 255, 0.1)) !important;
}

@keyframes spin {
    to {
        transform: rotate(360deg);
    }
}

@media (max-width: 1024px) {
    .desktop-icons {
        gap: 1rem !important;
    }

    .prompt-bar {
        padding: 0.5rem !important;
    }

    .running-apps {
        display: none !important;
    }
}

@media (max-width: 640px) {
    .desktop-icons {
        grid-template-columns: repeat(2, 4rem) !important;
    }
}
"#;
