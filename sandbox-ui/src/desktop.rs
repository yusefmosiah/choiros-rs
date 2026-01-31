//! Desktop Foundation - Theme-ready architecture
//!
//! Modular components with CSS token support. Themes can:
//! 1. Use CSS variables for quick styling
//! 2. Override entire component render functions
//! 3. Replace layout structure completely

use dioxus::prelude::*;
use shared_types::{WindowState, AppDefinition, DesktopState};

use crate::api::{
    fetch_desktop_state, open_window, close_window, focus_window,
    move_window, resize_window,
};
use crate::components::ChatView;

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
    
    // Track viewport size for responsive behavior
    use_effect(move || {
        spawn(async move {
            if let Ok((w, h)) = get_viewport_size().await {
                viewport.set((w, h));
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
            }).await;
        });
    });
    
    // Callbacks
    let open_app_window = use_callback(move |app: AppDefinition| {
        let desktop_id = desktop_id_signal.to_string();
        spawn(async move {
            match open_window(&desktop_id, &app.id, &app.name, None).await {
                Ok(window) => {
                    let window_id = window.id.clone();
                    desktop_state.write().as_mut().map(|s| {
                        s.windows.push(window);
                        s.active_window = Some(window_id);
                    });
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
                    desktop_state.write().as_mut().map(|s| {
                        s.windows.retain(|w| w.id != window_id);
                        if s.active_window == Some(window_id) {
                            s.active_window = s.windows.last().map(|w| w.id.clone());
                        }
                    });
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
                    desktop_state.write().as_mut().map(|s| {
                        s.active_window = Some(window_id.clone());
                        // Update z-index locally
                        let max_z = s.windows.iter().map(|w| w.z_index).max().unwrap_or(0);
                        if let Some(window) = s.windows.iter_mut().find(|w| w.id == window_id) {
                            window.z_index = max_z + 1;
                        }
                    });
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
    
    let current_state = desktop_state.read();
    let viewport_ref = viewport.read();
    let (vw, _vh) = *viewport_ref;
    let is_desktop = vw > 1024;
    
    rsx! {
        // Global CSS variables for theming
        style { {DEFAULT_TOKENS} }
        
        div {
            class: "desktop-shell",
            style: "min-height: 100vh; display: flex; flex-direction: column; overflow: hidden;",
            
            // Main workspace area
            div {
                class: "desktop-workspace",
                style: "flex: 1; display: flex; overflow: hidden; position: relative;",
                
                // App Dock (left side on desktop, hidden or bottom on mobile)
                if let Some(state) = current_state.as_ref() {
                    AppDock {
                        apps: state.apps.clone(),
                        on_open_app: open_app_window.clone(),
                        is_collapsed: !is_desktop,
                    }
                }
                
                // Window canvas
                div {
                    class: "window-canvas",
                    style: "flex: 1; position: relative; overflow: hidden;",
                    
                    if loading() {
                        LoadingState {}
                    } else if let Some(err) = error.read().as_ref() {
                        ErrorState { error: err.clone() }
                    } else if let Some(state) = current_state.as_ref() {
                        if state.windows.is_empty() {
                            EmptyState {}
                        } else {
                            for window in state.windows.iter() {
                                FloatingWindow {
                                    window: window.clone(),
                                    is_active: state.active_window.as_ref() == Some(&window.id),
                                    viewport: *viewport.read(),
                                    on_close: close_window_cb.clone(),
                                    on_focus: focus_window_cb.clone(),
                                    on_move: move_window_cb.clone(),
                                    on_resize: resize_window_cb.clone(),
                                }
                            }
                        }
                    }
                }
            }
            
            // Prompt Bar (always visible)
            PromptBar {
                connected: ws_connected(),
                on_submit: move |text: String| {
                    // TODO: Open chat with this prompt
                    dioxus_logger::tracing::info!("Prompt submitted: {}", text);
                },
            }
        }
    }
}

// ============================================================================
// App Dock - App launcher icons
// ============================================================================

#[component]
fn AppDock(
    apps: Vec<AppDefinition>,
    on_open_app: Callback<AppDefinition>,
    is_collapsed: bool,
) -> Element {
    if is_collapsed {
        // Mobile: Could show as bottom sheet or hamburger menu
        // For now, just hide and use prompt bar for app switching
        return rsx! { div { style: "display: none;" } };
    }
    
    rsx! {
        div {
            class: "app-dock",
            style: "width: 200px; display: flex; flex-direction: column; padding: 1rem; gap: 0.5rem; overflow-y: auto; border-right: 1px solid var(--border-color, #333); background: var(--dock-bg, #1a1a2e);",
            
            for app in apps {
                button {
                    class: "dock-app-btn",
                    style: "display: flex; align-items: center; gap: 0.75rem; padding: 0.75rem; background: transparent; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; color: var(--text-secondary, #9ca3af); transition: all 0.2s;",
                    onclick: move |_| on_open_app.call(app.clone()),
                    
                    span { style: "font-size: 1.5rem;", "{app.icon}" }
                    span { style: "font-size: 0.875rem; font-weight: 500;", "{app.name}" }
                }
            }
        }
    }
}

// ============================================================================
// Floating Window - Draggable, resizable window chrome
// ============================================================================

#[component]
fn FloatingWindow(
    window: WindowState,
    is_active: bool,
    viewport: (u32, u32),
    on_close: Callback<String>,
    on_focus: Callback<String>,
    on_move: Callback<(String, i32, i32)>,
    on_resize: Callback<(String, i32, i32)>,
) -> Element {
    let window_id = window.id.clone();
    let (vw, vh) = viewport;
    let is_mobile = vw <= 1024;
    
    // Responsive sizing
    let (width, height, x, y) = if is_mobile {
        // Mobile: Full screen with small margin
        (vw as i32 - 20, vh as i32 - 100, 10i32, 10i32)
    } else {
        // Desktop: Use window state, clamp to viewport
        let w = window.width.min(vw as i32 - 40);
        let h = window.height.min(vh as i32 - 120);
        let x = window.x.max(200).min(vw as i32 - w - 10); // Don't cover dock
        let y = window.y.max(10).min(vh as i32 - h - 60); // Don't cover prompt bar
        (w, h, x, y)
    };
    
    let z_index = window.z_index;
    let window_id_for_focus = window_id.clone();
    let window_id_for_drag = window_id.clone();
    let window_id_for_close = window_id.clone();
    let window_id_for_resize = window_id.clone();
    let on_move_drag = on_move.clone();
    
    rsx! {
        div {
            class: if is_active { "floating-window active" } else { "floating-window" },
            style: "position: absolute; left: {x}px; top: {y}px; width: {width}px; height: {height}px; z-index: {z_index}; display: flex; flex-direction: column; background: var(--window-bg, #1f2937); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-lg, 12px); overflow: hidden; box-shadow: var(--shadow-lg, 0 10px 40px rgba(0,0,0,0.5));",
            onclick: move |_| on_focus.call(window_id_for_focus.clone()),
            
            // Title bar (draggable)
            div {
                class: "window-titlebar",
                style: "display: flex; align-items: center; justify-content: space-between; padding: 0.75rem 1rem; background: var(--titlebar-bg, #111827); border-bottom: 1px solid var(--border-color, #374151); cursor: grab; user-select: none;",
                onmousedown: move |e| {
                    if !is_mobile {
                        start_drag(e, window_id_for_drag.clone(), on_move_drag.clone());
                    }
                },
                
                div {
                    style: "display: flex; align-items: center; gap: 0.5rem;",
                    span { style: "font-size: 1rem;", {get_app_icon(&window.app_id)} }
                    span { style: "font-weight: 500; color: var(--text-primary, white);", "{window.title}" }
                }
                
                button {
                    class: "window-close",
                    style: "width: 24px; height: 24px; display: flex; align-items: center; justify-content: center; background: transparent; color: var(--text-secondary, #9ca3af); border: none; border-radius: var(--radius-sm, 4px); cursor: pointer; font-size: 1.25rem; line-height: 1;",
                    onclick: move |e| {
                        e.stop_propagation();
                        on_close.call(window_id_for_close.clone());
                    },
                    "×"
                }
            }
            
            // Window content
            div {
                class: "window-content",
                style: "flex: 1; overflow: auto; padding: 1rem;",
                
                match window.app_id.as_str() {
                    "chat" => rsx! {
                        ChatView { actor_id: window.id.clone() }
                    },
                    _ => rsx! {
                        div {
                            style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted, #6b7280);",
                            "App not yet implemented"
                        }
                    }
                }
            }
            
            // Resize handle (desktop only)
            if !is_mobile {
                div {
                    class: "resize-handle",
                    style: "position: absolute; right: 0; bottom: 0; width: 16px; height: 16px; cursor: se-resize;",
                    onmousedown: move |e| {
                        start_resize(e, window_id_for_resize.clone(), on_resize.clone());
                    },
                }
            }
        }
    }
}

// ============================================================================
// Prompt Bar - Command input
// ============================================================================

#[component]
fn PromptBar(
    connected: bool,
    on_submit: Callback<String>,
) -> Element {
    let mut input_value = use_signal(|| String::new());
    
    rsx! {
        div {
            class: "prompt-bar",
            style: "display: flex; align-items: center; gap: 0.5rem; padding: 0.75rem 1rem; background: var(--promptbar-bg, #111827); border-top: 1px solid var(--border-color, #374151);",
            
            // Help button
            button {
                class: "prompt-help-btn",
                style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: var(--accent-bg, #3b82f6); color: white; border: none; border-radius: var(--radius-md, 8px); cursor: pointer; font-weight: 600;",
                onclick: move |_| {
                    // TODO: Show command palette
                },
                "?"
            }
            
            // Input field
            input {
                class: "prompt-input",
                style: "flex: 1; padding: 0.5rem 1rem; background: var(--input-bg, #1f2937); color: var(--text-primary, white); border: 1px solid var(--border-color, #374151); border-radius: var(--radius-md, 8px); font-size: 0.875rem; outline: none;",
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
            
            // Connection status
            div {
                class: if connected { "ws-status connected" } else { "ws-status" },
                style: if connected { 
                    "display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem 0.5rem; background: var(--success-bg, #10b981); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem;" 
                } else { 
                    "display: flex; align-items: center; gap: 0.25rem; padding: 0.25rem 0.5rem; background: var(--warning-bg, #f59e0b); color: white; border-radius: var(--radius-sm, 4px); font-size: 0.75rem;" 
                },
                
                span { if connected { "●" } else { "◐" } }
                span { if connected { "Connected" } else { "Connecting..." } }
            }
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

#[component]
fn EmptyState() -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--text-muted, #6b7280);",
            p { "No windows open" }
            p { style: "font-size: 0.875rem; margin-top: 0.5rem;", "Click an app in the dock to get started" }
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
    --accent-text: #ffffff;
    --border-color: #334155;
    
    /* Semantic colors */
    --window-bg: var(--bg-secondary);
    --titlebar-bg: var(--bg-primary);
    --dock-bg: var(--bg-secondary);
    --promptbar-bg: var(--bg-primary);
    --input-bg: var(--bg-secondary);
    --hover-bg: rgba(255, 255, 255, 0.1);
    --danger-bg: #ef4444;
    --danger-text: #ef4444;
    --success-bg: #10b981;
    --warning-bg: #f59e0b;
    
    /* Spacing & Radius */
    --radius-sm: 4px;
    --radius-md: 8px;
    --radius-lg: 12px;
    
    /* Shadows */
    --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
    --shadow-md: 0 4px 6px rgba(0, 0, 0, 0.4);
    --shadow-lg: 0 10px 40px rgba(0, 0, 0, 0.5);
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
"#;

// ============================================================================
// Helper Functions
// ============================================================================

fn get_app_icon(app_id: &str) -> &'static str {
    match app_id {
        "chat" => "💬",
        _ => "📱",
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
            desktop_state.write().as_mut().map(|s| {
                s.windows.push(window);
            });
        }
        WsEvent::WindowClosed(window_id) => {
            desktop_state.write().as_mut().map(|s| {
                s.windows.retain(|w| w.id != window_id);
            });
        }
        WsEvent::WindowMoved { window_id, x, y } => {
            desktop_state.write().as_mut().map(|s| {
                if let Some(window) = s.windows.iter_mut().find(|w| w.id == window_id) {
                    window.x = x;
                    window.y = y;
                }
            });
        }
        WsEvent::WindowResized { window_id, width, height } => {
            desktop_state.write().as_mut().map(|s| {
                if let Some(window) = s.windows.iter_mut().find(|w| w.id == window_id) {
                    window.width = width as i32;
                    window.height = height as i32;
                }
            });
        }
        WsEvent::WindowFocused(window_id) => {
            desktop_state.write().as_mut().map(|s| {
                s.active_window = Some(window_id.clone());
            });
        }
    }
}

// Drag and resize helpers (WASM interop)
fn start_drag(e: Event<MouseData>, window_id: String, on_move: Callback<(String, i32, i32)>) {
    // TODO: Implement drag via JS interop
    // This would capture mouse movement and call on_move with new coordinates
    let _ = (e, window_id, on_move);
}

fn start_resize(e: Event<MouseData>, window_id: String, on_resize: Callback<(String, i32, i32)>) {
    // TODO: Implement resize via JS interop
    let _ = (e, window_id, on_resize);
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
    WindowMoved { window_id: String, x: i32, y: i32 },
    WindowResized { window_id: String, width: u32, height: u32 },
    WindowFocused(String),
}

async fn connect_websocket<F>(_desktop_id: &str, _on_event: F) 
where
    F: FnMut(WsEvent) + 'static,
{
    // TODO: Implement WebSocket connection
    // For now, just mark as connected
}
