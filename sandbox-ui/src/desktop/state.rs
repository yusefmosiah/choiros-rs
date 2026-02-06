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
                    window.width = width as i32;
                    window.height = height as i32;
                }
            }
        }
        WsEvent::WindowFocused(window_id) => {
            if let Some(state) = desktop_state.write().as_mut() {
                state.active_window = Some(window_id);
            }
        }
        WsEvent::Pong => {}
        WsEvent::Error(_) => {}
    }
}

pub fn push_window_and_activate(state: &mut DesktopState, window: WindowState) {
    let window_id = window.id.clone();
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

pub fn find_chat_window_id(state: &Option<DesktopState>) -> Option<String> {
    state.as_ref().and_then(|desktop| {
        desktop
            .windows
            .iter()
            .find(|window| window.app_id == "chat")
            .map(|window| window.id.clone())
    })
}
