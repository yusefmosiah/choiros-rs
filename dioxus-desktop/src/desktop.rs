//! Desktop Foundation - Theme-ready architecture

use dioxus::prelude::*;

mod actions;
mod apps;
mod components;
mod effects;
mod shell;
pub mod state;
mod theme;
mod ws;

pub use shell::DesktopShell;

#[component]
pub fn Desktop(desktop_id: String) -> Element {
    rsx! {
        DesktopShell { desktop_id }
    }
}
