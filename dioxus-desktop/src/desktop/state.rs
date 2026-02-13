use dioxus::prelude::{Signal, WritableExt};
use shared_types::{DesktopState, WindowState};

use crate::desktop::ws::WsEvent;

pub fn apply_ws_event(
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
            if let Some(state) = desktop_state.write().as_mut() {
                state.windows.retain(|w| w.id != window.id);
                state.windows.push(window);
            }
        }
        WsEvent::WindowClosed(window_id) => {
            if let Some(state) = desktop_state.write().as_mut() {
                state.windows.retain(|w| w.id != window_id);
            }
        }
        WsEvent::WindowMoved { window_id, x, y } => {
            if let Some(state) = desktop_state.write().as_mut() {
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
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
            if let Some(state) = desktop_state.write().as_mut() {
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.width = width;
                    window.height = height;
                }
            }
        }
        WsEvent::WindowFocused(window_id) => {
            if let Some(state) = desktop_state.write().as_mut() {
                state.active_window = Some(window_id.clone());
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.minimized = false;
                }
            }
        }
        WsEvent::WindowMinimized(window_id) => {
            if let Some(state) = desktop_state.write().as_mut() {
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.minimized = true;
                    window.maximized = false;
                }

                if state.active_window.as_deref() == Some(&window_id) {
                    state.active_window = state
                        .windows
                        .iter()
                        .filter(|w| !w.minimized)
                        .max_by_key(|w| w.z_index)
                        .map(|w| w.id.clone());
                }
            }
        }
        WsEvent::WindowMaximized {
            window_id,
            x,
            y,
            width,
            height,
        } => {
            if let Some(state) = desktop_state.write().as_mut() {
                let next_z = state.windows.iter().map(|w| w.z_index).max().unwrap_or(0) + 1;
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.minimized = false;
                    window.maximized = true;
                    window.x = x;
                    window.y = y;
                    window.width = width;
                    window.height = height;
                    window.z_index = next_z;
                }
                state.active_window = Some(window_id);
            }
        }
        WsEvent::WindowRestored {
            window_id,
            x,
            y,
            width,
            height,
            maximized,
        } => {
            if let Some(state) = desktop_state.write().as_mut() {
                if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
                    window.minimized = false;
                    window.maximized = maximized;
                    window.x = x;
                    window.y = y;
                    window.width = width;
                    window.height = height;
                }
                state.active_window = Some(window_id);
            }
        }
        WsEvent::Pong => {}
        WsEvent::Error(_) => {}
        WsEvent::Telemetry { .. } => {
            // Telemetry events are handled separately by the prompt bar
            // They don't modify desktop state
        }
        WsEvent::DocumentUpdate { .. } => {
            // Document update events are handled separately by the run view
            // They don't modify desktop state
        }
    }
}

pub fn push_window_and_activate(state: &mut DesktopState, window: WindowState) {
    let window_id = window.id.clone();
    state.windows.retain(|w| w.id != window_id);
    state.windows.push(window);
    state.active_window = Some(window_id);
}

pub fn remove_window_and_reselect_active(state: &mut DesktopState, window_id: &str) {
    state.windows.retain(|w| w.id != window_id);

    if state.active_window.as_deref() == Some(window_id) {
        state.active_window = state.windows.last().map(|w| w.id.clone());
    }
}

pub fn focus_window_and_raise_z(state: &mut DesktopState, window_id: &str) {
    state.active_window = Some(window_id.to_string());

    let max_z = state.windows.iter().map(|w| w.z_index).max().unwrap_or(0);
    if let Some(window) = state.windows.iter_mut().find(|w| w.id == window_id) {
        window.z_index = max_z + 1;
    }
}
