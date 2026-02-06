use dioxus::prelude::*;
use shared_types::{AppDefinition, DesktopState};

use crate::desktop::components::desktop_icons::DesktopIcons;
use crate::desktop::components::status_views::{ErrorState, LoadingState};
use crate::desktop_window::FloatingWindow;

#[component]
pub fn WorkspaceCanvas(
    desktop_id: String,
    apps: Vec<AppDefinition>,
    on_open_app: Callback<AppDefinition>,
    is_mobile: bool,
    loading: Signal<bool>,
    error: Signal<Option<String>>,
    state: Signal<Option<DesktopState>>,
    viewport: Signal<(u32, u32)>,
    on_close: Callback<String>,
    on_focus: Callback<String>,
    on_move: Callback<(String, i32, i32)>,
    on_resize: Callback<(String, i32, i32)>,
    on_minimize: Callback<String>,
    on_maximize: Callback<String>,
    on_restore: Callback<String>,
) -> Element {
    let state_value = state.read().clone();
    let viewport_value = *viewport.read();

    rsx! {
        div {
            class: "desktop-workspace",
            style: "flex: 1; display: flex; flex-direction: column; overflow: hidden; position: relative;",

            if state_value.is_some() {
                DesktopIcons {
                    apps,
                    on_open_app,
                    is_mobile,
                }
            }

            div {
                class: "window-canvas",
                style: "flex: 1; position: relative; overflow: hidden;",

                if loading() {
                    LoadingState {}
                } else if let Some(err) = error.read().clone() {
                    ErrorState { error: err }
                } else if let Some(desktop_state) = state_value {
                    for window in desktop_state.windows.iter().filter(|w| !w.minimized) {
                        FloatingWindow {
                            key: "{window.id}",
                            window: window.clone(),
                            desktop_id: desktop_id.clone(),
                            is_active: desktop_state.active_window.as_ref() == Some(&window.id),
                            viewport: viewport_value,
                            on_close,
                            on_focus,
                            on_move,
                            on_resize,
                            on_minimize,
                            on_maximize,
                            on_restore,
                        }
                    }
                }
            }
        }
    }
}
