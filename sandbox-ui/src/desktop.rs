//! Desktop Foundation - Theme-ready architecture
//!
//! Modular components with CSS token support. Themes can:
//! 1. Use CSS variables for quick styling
//! 2. Override entire component render functions
//! 3. Replace layout structure completely

use dioxus::prelude::*;
use shared_types::{AppDefinition, DesktopState, WindowState};

use crate::api::{
    close_window, fetch_desktop_state, fetch_user_theme_preference, focus_window, move_window,
    open_window, register_app, resize_window, send_chat_message, update_user_theme_preference,
};
use crate::desktop_window::FloatingWindow;

// ============================================================================
// Desktop Component - Main Container
// ============================================================================

#[component]
pub fn Desktop(desktop_id: String) -> Element {
    let mut desktop_state = use_signal(|| None::<DesktopState>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut ws_connected = use_signal(|| false);
    let desktop_id_signal = use_signal(|| desktop_id.clone());
    let mut viewport = use_signal(|| (1920u32, 1080u32));
    let mut apps_registered = use_signal(|| false);
    let mut theme_initialized = use_signal(|| false);
    let mut current_theme = use_signal(|| "dark".to_string());

    // Track viewport size for responsive behavior
    use_effect(move || {
        spawn(async move {
            if let Ok((w, h)) = get_viewport_size().await {
                viewport.set((w, h));
            }
        });
    });

    // Initialize theme from cache first, then backend user-global preference.
    use_effect(move || {
        if theme_initialized() {
            return;
        }
        theme_initialized.set(true);

        if let Some(theme) = get_cached_theme_preference() {
            apply_theme_to_document(&theme);
            current_theme.set(theme);
        }

        spawn(async move {
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
                        apply_theme_to_document("dark");
                        current_theme.set("dark".to_string());
                    }
                }
            }
        });
    });

    let toggle_theme = use_callback(move |_| {
        let next_theme = if current_theme() == "light" {
            "dark".to_string()
        } else {
            "light".to_string()
        };
        current_theme.set(next_theme.clone());
        apply_theme_to_document(&next_theme);
        set_cached_theme_preference(&next_theme);

        spawn(async move {
            if let Err(e) = update_user_theme_preference("user-1", &next_theme).await {
                dioxus_logger::tracing::warn!("Failed to persist theme preference: {}", e);
            }
        });
    });

    // Load initial desktop state
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

    // WebSocket connection for real-time updates
    use_effect(move || {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            connect_websocket(&desktop_id, move |event| {
                handle_ws_event(event, &mut desktop_state, &mut ws_connected);
            })
            .await;
        });
    });

    // Callbacks
    let open_app_window = use_callback(move |app: AppDefinition| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            match open_window(&desktop_id, &app.id, &app.name, None).await {
                Ok(window) => {
                    let window_id = window.id.clone();
                    if let Some(s) = desktop_state.write().as_mut() {
                        s.windows.push(window);
                        s.active_window = Some(window_id);
                    }
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to open window: {}", e);
                }
            }
        });
    });

    let close_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            match close_window(&desktop_id, &window_id).await {
                Ok(_) => {
                    if let Some(s) = desktop_state.write().as_mut() {
                        s.windows.retain(|w| w.id != window_id);
                        if s.active_window == Some(window_id) {
                            s.active_window = s.windows.last().map(|w| w.id.clone());
                        }
                    }
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to close window: {}", e);
                }
            }
        });
    });

    let focus_window_cb = use_callback(move |window_id: String| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            match focus_window(&desktop_id, &window_id).await {
                Ok(_) => {
                    if let Some(s) = desktop_state.write().as_mut() {
                        s.active_window = Some(window_id.clone());
                        // Update z-index locally
                        let max_z = s.windows.iter().map(|w| w.z_index).max().unwrap_or(0);
                        if let Some(window) = s.windows.iter_mut().find(|w| w.id == window_id) {
                            window.z_index = max_z + 1;
                        }
                    }
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to focus window: {}", e);
                }
            }
        });
    });

    let move_window_cb = use_callback(move |(window_id, x, y): (String, i32, i32)| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            if let Err(e) = move_window(&desktop_id, &window_id, x, y).await {
                dioxus_logger::tracing::error!("Failed to move window: {}", e);
            }
        });
    });

    let resize_window_cb = use_callback(move |(window_id, w, h): (String, i32, i32)| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            if let Err(e) = resize_window(&desktop_id, &window_id, w, h).await {
                dioxus_logger::tracing::error!("Failed to resize window: {}", e);
            }
        });
    });

    // Handle prompt bar submission - opens/focuses chat and sends message
    let handle_prompt_submit = use_callback(move |text: String| {
        let desktop_id = desktop_id_signal.to_string();
        // Clone the signal setter for use inside async block
        let mut state_signal = desktop_state;

        spawn(async move {
            // Try to find existing chat window by fetching fresh state
            let chat_window_id = state_signal.read().as_ref().and_then(|s| {
                s.windows
                    .iter()
                    .find(|w| w.app_id == "chat")
                    .map(|w| w.id.clone())
            });

            if let Some(window_id) = chat_window_id {
                // Focus existing chat window
                let _ = focus_window(&desktop_id, &window_id).await;
                // Send message to chat
                let _ = send_chat_message(&window_id, "user-1", &text).await;
            } else {
                // Open new chat window
                match open_window(&desktop_id, "chat", "Chat", None).await {
                    Ok(window) => {
                        let window_id = window.id.clone();
                        if let Some(s) = state_signal.write().as_mut() {
                            s.windows.push(window);
                            s.active_window = Some(window_id.clone());
                        }
                        // Send message to new chat window
                        let _ = send_chat_message(&window_id, "user-1", &text).await;
                    }
                    Err(e) => {
                        dioxus_logger::tracing::error!("Failed to open chat window: {}", e);
                    }
                }
            }
        });
    });

    let current_state = desktop_state.read();
    let viewport_ref = viewport.read();
    let (vw, _vh) = *viewport_ref;
    let is_desktop = vw > 1024;

    // Core apps for desktop icons
    let core_apps = vec![
        AppDefinition {
            id: "chat".to_string(),
            name: "Chat".to_string(),
            icon: "💬".to_string(),
            component_code: "ChatApp".to_string(),
            default_width: 600,
            default_height: 500,
        },
        AppDefinition {
            id: "writer".to_string(),
            name: "Writer".to_string(),
            icon: "📝".to_string(),
            component_code: "WriterApp".to_string(),
            default_width: 800,
            default_height: 600,
        },
        AppDefinition {
            id: "terminal".to_string(),
            name: "Terminal".to_string(),
            icon: "🖥️".to_string(),
            component_code: "TerminalApp".to_string(),
            default_width: 700,
            default_height: 450,
        },
        AppDefinition {
            id: "files".to_string(),
            name: "Files".to_string(),
            icon: "📁".to_string(),
            component_code: "FilesApp".to_string(),
            default_width: 700,
            default_height: 500,
        },
    ];

    // Register core apps in backend (best-effort)
    {
        let desktop_id = desktop_id_signal.to_string();
        let apps = core_apps.clone();
        use_effect(move || {
            if apps_registered() {
                return;
            }
            apps_registered.set(true);
            let desktop_id_inner = desktop_id.clone();
            let apps_inner = apps.clone();
            spawn(async move {
                for app in apps_inner {
                    let _ = register_app(&desktop_id_inner, &app).await;
                }
            });
        });
    }

    rsx! {
            // Global CSS variables for theming
            style { {DEFAULT_TOKENS} }

            link {
                rel: "stylesheet",
                href: "/xterm.css",
            }
            script { src: "/xterm.js" }
            script { src: "/xterm-addon-fit.js" }
            script { src: "/terminal.js" }

            div {
                class: "desktop-shell",
                style: "min-height: 100vh; display: flex; flex-direction: column; overflow: hidden;",

                // Main workspace area (full width, no sidebar)
                div {
                    class: "desktop-workspace",
                    style: "flex: 1; display: flex; flex-direction: column; overflow: hidden; position: relative;",

                    // Desktop icons (grid layout)
    if let Some(_state) = current_state.as_ref() {
                        DesktopIcons {
                            apps: core_apps,
                            on_open_app: open_app_window,
                            is_mobile: !is_desktop,
                        }
                    }

                    // Window canvas (full width, positioned over icons)
                    div {
                        class: "window-canvas",
                        style: "flex: 1; position: relative; overflow: hidden;",

                        if loading() {
                            LoadingState {}
                        } else if let Some(err) = error.read().as_ref() {
                            ErrorState { error: err.clone() }
                        } else if let Some(state) = current_state.as_ref() {
                            for window in state.windows.iter() {
                                FloatingWindow {
                                    window: window.clone(),
                                    is_active: state.active_window.as_ref() == Some(&window.id),
                                    viewport: *viewport.read(),
                                    on_close: close_window_cb,
                                    on_focus: focus_window_cb,
                                    on_move: move_window_cb,
                                    on_resize: resize_window_cb,
                                }
                            }
                        }
                    }
                }

                // Prompt Bar with running app indicators
                if let Some(state) = current_state.as_ref() {
                    PromptBar {
                        connected: ws_connected(),
                        windows: state.windows.clone(),
                        active_window: state.active_window.clone(),
                        on_submit: handle_prompt_submit,
                        on_focus_window: focus_window_cb,
                        current_theme: current_theme(),
                        on_toggle_theme: toggle_theme,
                    }
                } else {
                    PromptBar {
                        connected: ws_connected(),
                        windows: vec![],
                        active_window: None,
                        on_submit: handle_prompt_submit,
                        on_focus_window: focus_window_cb,
                        current_theme: current_theme(),
                        on_toggle_theme: toggle_theme,
                    }
                }
            }
        }
}

// ============================================================================
// Desktop Icons - App launcher on desktop background
// ============================================================================

#[component]
fn DesktopIcons(
    apps: Vec<AppDefinition>,
    on_open_app: Callback<AppDefinition>,
    is_mobile: bool,
) -> Element {
    // Grid layout: 4 columns on desktop, 2 on mobile
    let columns = if is_mobile { 2 } else { 4 };
    let icon_size = if is_mobile { "4rem" } else { "5rem" };

    rsx! {
        div {
            class: "desktop-icons",
            style: "position: absolute; top: 1rem; left: 1rem; z-index: 1; display: grid; grid-template-columns: repeat({columns}, {icon_size}); gap: 1.5rem; padding: 1rem;",

            for app in apps {
                DesktopIcon {
                    app: app.clone(),
                    on_open_app: on_open_app,
                    is_mobile,
                }
            }
        }
    }
}

#[component]
fn DesktopIcon(
    app: AppDefinition,
    on_open_app: Callback<AppDefinition>,
    is_mobile: bool,
) -> Element {
    let icon_size = if is_mobile { "3rem" } else { "3.5rem" };
    let font_size = if is_mobile { "2rem" } else { "2.5rem" };
    let mut last_click_time = use_signal(|| 0i64);
    let mut is_pressed = use_signal(|| false);

    // Extract fields for use in both closure and render
    let app_for_closure = app.clone();
    let app_icon = app.icon.clone();
    let app_name = app.name.clone();

    let handle_click = move |_| {
        let now = js_sys::Date::now() as i64;
        let last = *last_click_time.read();

        if now - last >= 500 {
            on_open_app.call(app_for_closure.clone());
            last_click_time.set(now);
        }

        // Always set pressed state for visual feedback
        is_pressed.set(true);
        let mut is_pressed_clone = is_pressed;
        spawn(async move {
            wasm_bindgen_futures::JsFuture::from(js_sys::Promise::new(&mut |resolve, _| {
                web_sys::window()
                    .unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 150)
                    .unwrap();
            }))
            .await
            .unwrap();
            is_pressed_clone.set(false);
        });
    };

    // Visual feedback styles
    let bg_opacity = if *is_pressed.read() { "0.95" } else { "0.8" };
    let scale = if *is_pressed.read() { "0.95" } else { "1.0" };
    let border_color = if *is_pressed.read() {
        "#60a5fa"
    } else {
        "#334155"
    };
    let shadow = if *is_pressed.read() {
        "0 2px 12px rgba(96, 165, 250, 0.5)"
    } else {
        "none"
    };

    rsx! {
        button {
            class: "desktop-icon",
            style: "display: flex; flex-direction: column; align-items: center; gap: 0.5rem; padding: 0.75rem; background: transparent; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; transition: all 0.15s ease-out; transform: scale({scale});",
            onclick: handle_click,
            onmouseleave: move |_| is_pressed.set(false),

            div {
                style: "width: {icon_size}; height: {icon_size}; display: flex; align-items: center; justify-content: center; background: var(--dock-bg, rgba(30, 41, 59, {bg_opacity})); border-radius: var(--radius-lg, 12px); backdrop-filter: blur(8px); border: 1px solid {border_color}; box-shadow: {shadow}; transition: all 0.15s ease-out;",
                span { style: "font-size: {font_size}; pointer-events: none; user-select: none;", "{app_icon}" }
            }
            span {
                style: "font-size: 0.75rem; color: var(--text-secondary, #94a3b8); text-align: center; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-shadow: 0 1px 2px rgba(0,0,0,0.5); pointer-events: none; user-select: none;",
                "{app_name}"
            }
        }
    }
}

// ============================================================================
// Prompt Bar - Command input with running app indicators
// ============================================================================

#[component]
fn PromptBar(
    connected: bool,
    windows: Vec<WindowState>,
    active_window: Option<String>,
    on_submit: Callback<String>,
    on_focus_window: Callback<String>,
    current_theme: String,
    on_toggle_theme: Callback<()>,
) -> Element {
    let mut input_value = use_signal(String::new);

    rsx! {
        div {
            class: "prompt-bar",
            style: "display: flex; align-items: center; gap: 0.5rem; padding: 0.75rem 1rem; background: var(--promptbar-bg, #111827); border-top: 1px solid var(--border-color, #374151);",

            // Help button
            button {
                class: "prompt-help-btn",
                style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--accent-bg, #3b82f6); color: white; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; font-weight: 600; flex-shrink: 0;",
                onclick: move |_| {
                    // TODO: Show command palette
                },
                "?"
            }

            button {
                class: "prompt-theme-btn",
                style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; flex-shrink: 0;",
                onclick: move |_| on_toggle_theme.call(()),
                title: "Toggle theme",
                if current_theme == "dark" {
                    "☀️"
                } else {
                    "🌙"
                }
            }

            // Input field
            input {
                class: "prompt-input",
                style: "flex: 1; padding: 0.5rem 1rem; background: var(--input-bg, #1f2937); color: var(--text-primary, white); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); font-size: 0.875rem; outline: none; min-width: 0;",
                placeholder: "Ask anything, paste URL, or type ? for commands...",
                value: "{input_value}",
                oninput: move |e| input_value.set(e.value().clone()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        let text = input_value.to_string();
                        if !text.is_empty() {
                            on_submit.call(text);
                            input_value.set(String::new());
                        }
                    }
                }
            }

            // Running app indicators (right side)
            if !windows.is_empty() {
                div {
                    class: "running-apps",
                    style: "display: flex; align-items: center; gap: 0.25rem; flex-shrink: 0;",

                    for window in windows.iter() {
                        RunningAppIndicator {
                            window: window.clone(),
                            is_active: active_window.as_ref() == Some(&window.id),
                            on_focus: on_focus_window,
                        }
                    }
                }
            }

            // Connection status
            div {
                class: if connected { "ws-status connected" } else { "ws-status" },
                style: if connected {
                    "display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem 0.5rem; background: var(--success-bg, #10b981); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; flex-shrink: 0;"
                } else {
                    "display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem 0.5rem; background: var(--warning-bg, #f59e0b); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem; flex-shrink: 0;"
                },

                span { if connected { "●" } else { "◐" } }
                span { if connected { "Connected" } else { "Connecting..." } }
            }
        }
    }
}

#[component]
fn RunningAppIndicator(
    window: WindowState,
    is_active: bool,
    on_focus: Callback<String>,
) -> Element {
    let icon = get_app_icon(&window.app_id);
    let window_id = window.id.clone();

    rsx! {
        button {
            class: if is_active { "running-app active" } else { "running-app" },
            style: if is_active {
                "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--accent-bg, #3b82f6); color: white; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 1.25rem;"
            } else {
                "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--window-bg, #1f2937); color: var(--text-secondary, #9ca3af); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); cursor: pointer; font-size: 1.25rem;"
            },
            onclick: move |_| on_focus.call(window_id.clone()),
            title: "{window.title}",
            "{icon}"
        }
    }
}

// ============================================================================
// Helper Components
// ============================================================================

#[component]
fn LoadingState() -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted, #6b7280);",
            "Loading desktop..."
        }
    }
}

#[component]
fn ErrorState(error: String) -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--danger-text, #ef4444); padding: 2rem; text-align: center;",
            p { style: "font-weight: 500; margin-bottom: 0.5rem;", "Error loading desktop" }
            p { style: "font-size: 0.875rem; color: var(--text-secondary, #9ca3af);", "{error}" }
        }
    }
}

// ============================================================================
// Default CSS Tokens (themes can override)
// ============================================================================

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

body {
    margin: 0;
    padding: 0;
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
}

/* Desktop icon hover effect */
.desktop-icon:hover {
    background: var(--hover-bg, rgba(255, 255, 255, 0.1));
}

.desktop-icon:hover div {
    background: var(--window-bg, #1f2937);
    transform: scale(1.05);
}

/* Running app indicator hover */
.running-app:hover {
    background: var(--hover-bg, rgba(255, 255, 255, 0.1)) !important;
}

/* Mobile responsive adjustments */
@media (max-width: 1024px) {
    .desktop-icons {
        gap: 1rem !important;
    }
    
    .prompt-bar {
        padding: 0.5rem !important;
    }
    
    .running-apps {
        display: none !important; /* Hide running apps on small mobile */
    }
}

@media (max-width: 640px) {
    .desktop-icons {
        grid-template-columns: repeat(2, 4rem) !important;
    }
}
"#;

// ============================================================================
// Helper Functions
// ============================================================================

fn get_app_icon(app_id: &str) -> &'static str {
    match app_id {
        "chat" => "💬",
        "writer" => "📝",
        "terminal" => "🖥️",
        "files" => "📁",
        _ => "📱",
    }
}

fn apply_theme_to_document(theme: &str) {
    if !matches!(theme, "light" | "dark") {
        return;
    }
    if let Some(document) = web_sys::window().and_then(|w| w.document()) {
        if let Some(root) = document.document_element() {
            let _ = root.set_attribute("data-theme", theme);
        }
    }
}

fn get_cached_theme_preference() -> Option<String> {
    web_sys::window()
        .and_then(|window| window.local_storage().ok().flatten())
        .and_then(|storage| storage.get_item("theme-preference").ok().flatten())
        .filter(|theme| matches!(theme.as_str(), "light" | "dark"))
}

fn set_cached_theme_preference(theme: &str) {
    if !matches!(theme, "light" | "dark") {
        return;
    }
    if let Some(storage) =
        web_sys::window().and_then(|window| window.local_storage().ok().flatten())
    {
        let _ = storage.set_item("theme-preference", theme);
    }
}

fn handle_ws_event(
    event: WsEvent,
    desktop_state: &mut Signal<Option<DesktopState>>,
    ws_connected: &mut Signal<bool>,
) {
    match event {
        WsEvent::Connected => {
            ws_connected.set(true);
        }
        WsEvent::Disconnected => {
            ws_connected.set(false);
        }
        WsEvent::DesktopStateUpdate(state) => {
            desktop_state.set(Some(state));
        }
        WsEvent::WindowOpened(window) => {
            if let Some(s) = desktop_state.write().as_mut() {
                s.windows.push(window);
            }
        }
        WsEvent::WindowClosed(window_id) => {
            if let Some(s) = desktop_state.write().as_mut() {
                s.windows.retain(|w| w.id != window_id);
            }
        }
        WsEvent::WindowMoved { window_id, x, y } => {
            if let Some(s) = desktop_state.write().as_mut() {
                if let Some(window) = s.windows.iter_mut().find(|w| w.id == window_id) {
                    window.x = x;
                    window.y = y;
                }
            }
        }
        WsEvent::WindowResized {
            window_id,
            width,
            height,
        } => {
            if let Some(s) = desktop_state.write().as_mut() {
                if let Some(window) = s.windows.iter_mut().find(|w| w.id == window_id) {
                    window.width = width as i32;
                    window.height = height as i32;
                }
            }
        }
        WsEvent::WindowFocused(window_id) => {
            if let Some(s) = desktop_state.write().as_mut() {
                s.active_window = Some(window_id.clone());
            }
        }
    }
}

async fn get_viewport_size() -> Result<(u32, u32), String> {
    // TODO: Get actual viewport size via web-sys
    Ok((1920, 1080))
}

// WebSocket types and connection
#[derive(Debug, Clone)]
enum WsEvent {
    Connected,
    Disconnected,
    DesktopStateUpdate(DesktopState),
    WindowOpened(WindowState),
    WindowClosed(String),
    WindowMoved {
        window_id: String,
        x: i32,
        y: i32,
    },
    WindowResized {
        window_id: String,
        width: u32,
        height: u32,
    },
    WindowFocused(String),
}

/// Convert HTTP URL to WebSocket URL
fn http_to_ws_url(http_url: &str) -> String {
    if http_url.starts_with("http://") {
        http_url.replace("http://", "ws://")
    } else if http_url.starts_with("https://") {
        http_url.replace("https://", "wss://")
    } else if http_url.is_empty() {
        // Same origin - use current protocol
        let protocol = web_sys::window()
            .and_then(|w| w.location().protocol().ok())
            .unwrap_or_else(|| "http:".to_string());
        let host = web_sys::window()
            .and_then(|w| w.location().host().ok())
            .unwrap_or_else(|| "localhost".to_string());

        if protocol == "https:" {
            format!("wss://{host}")
        } else {
            format!("ws://{host}")
        }
    } else {
        format!("ws://{http_url}")
    }
}

async fn connect_websocket<F>(desktop_id: &str, mut on_event: F)
where
    F: FnMut(WsEvent) + 'static,
{
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::prelude::*;
    use web_sys::{MessageEvent, WebSocket};

    // Get WebSocket base URL
    let api_base = crate::api::api_base();
    let ws_base = http_to_ws_url(api_base);

    // Connect to the general desktop WebSocket endpoint at /ws
    // Then send a subscribe message for the desktop_id
    let ws_url = format!("{ws_base}/ws");

    dioxus_logger::tracing::info!("Connecting to WebSocket: {}", ws_url);

    // Create WebSocket
    let ws = match WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            dioxus_logger::tracing::error!("Failed to create WebSocket: {:?}", e);
            on_event(WsEvent::Disconnected);
            return;
        }
    };

    // Wrap the callback in Rc<RefCell> to share it across handlers
    let on_event_rc = Rc::new(RefCell::new(on_event));
    let on_event_open = on_event_rc.clone();
    let on_event_close = on_event_rc.clone();
    let desktop_id_clone = desktop_id.to_string();
    let ws_clone = ws.clone();

    // Set up onopen handler
    let onopen_callback = Closure::wrap(Box::new(move |_e: wasm_bindgen::JsValue| {
        dioxus_logger::tracing::info!("WebSocket connected");
        on_event_open.borrow_mut()(WsEvent::Connected);

        // Send subscribe message for this desktop
        let subscribe_msg =
            format!("{{\"type\":\"subscribe\",\"desktop_id\":\"{desktop_id_clone}\"}}");
        let _ = ws_clone.send_with_str(&subscribe_msg);
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));
    onopen_callback.forget();

    // Set up onmessage handler
    let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
            let text_str = text.as_string().unwrap_or_default();
            dioxus_logger::tracing::debug!("WebSocket message: {}", text_str);

            // Parse the message and handle different types
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text_str) {
                if let Some(msg_type) = json.get("type").and_then(|t| t.as_str()) {
                    match msg_type {
                        "pong" => {
                            dioxus_logger::tracing::debug!("WebSocket pong received");
                        }
                        "desktop_state" => {
                            if let Ok(state) = serde_json::from_value::<DesktopState>(
                                json.get("desktop").cloned().unwrap_or_default(),
                            ) {
                                on_event_rc.borrow_mut()(WsEvent::DesktopStateUpdate(state));
                            }
                        }
                        "window_opened" => {
                            if let Ok(window) = serde_json::from_value::<WindowState>(
                                json.get("window").cloned().unwrap_or_default(),
                            ) {
                                on_event_rc.borrow_mut()(WsEvent::WindowOpened(window));
                            }
                        }
                        "window_closed" => {
                            if let Some(window_id) = json.get("window_id").and_then(|v| v.as_str())
                            {
                                on_event_rc.borrow_mut()(WsEvent::WindowClosed(
                                    window_id.to_string(),
                                ));
                            }
                        }
                        "window_moved" => {
                            if let (Some(window_id), Some(x), Some(y)) = (
                                json.get("window_id").and_then(|v| v.as_str()),
                                json.get("x").and_then(|v| v.as_i64()),
                                json.get("y").and_then(|v| v.as_i64()),
                            ) {
                                on_event_rc.borrow_mut()(WsEvent::WindowMoved {
                                    window_id: window_id.to_string(),
                                    x: x as i32,
                                    y: y as i32,
                                });
                            }
                        }
                        "window_resized" => {
                            if let (Some(window_id), Some(width), Some(height)) = (
                                json.get("window_id").and_then(|v| v.as_str()),
                                json.get("width").and_then(|v| v.as_u64()),
                                json.get("height").and_then(|v| v.as_u64()),
                            ) {
                                on_event_rc.borrow_mut()(WsEvent::WindowResized {
                                    window_id: window_id.to_string(),
                                    width: width as u32,
                                    height: height as u32,
                                });
                            }
                        }
                        "window_focused" => {
                            if let Some(window_id) = json.get("window_id").and_then(|v| v.as_str())
                            {
                                on_event_rc.borrow_mut()(WsEvent::WindowFocused(
                                    window_id.to_string(),
                                ));
                            }
                        }
                        "error" => {
                            if let Some(msg) = json.get("message").and_then(|v| v.as_str()) {
                                dioxus_logger::tracing::error!("WebSocket error message: {}", msg);
                            }
                        }
                        _ => {
                            dioxus_logger::tracing::warn!(
                                "Unknown WebSocket message type: {}",
                                msg_type
                            );
                        }
                    }
                }
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));
    onmessage_callback.forget();

    // Set up onclose handler
    let onclose_callback = Closure::wrap(Box::new(move |_e: wasm_bindgen::JsValue| {
        dioxus_logger::tracing::info!("WebSocket disconnected");
        on_event_close.borrow_mut()(WsEvent::Disconnected);
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));
    onclose_callback.forget();

    // Set up onerror handler
    let onerror_callback = Closure::wrap(Box::new(move |e: wasm_bindgen::JsValue| {
        dioxus_logger::tracing::error!("WebSocket error: {:?}", e);
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));
    onerror_callback.forget();
}
