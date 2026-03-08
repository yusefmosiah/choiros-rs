pub mod api;
pub mod auth;
pub mod components;
pub mod desktop;
pub mod desktop_window;
pub mod interop;
pub mod terminal;
pub mod viewers;

pub use api::*;
pub use auth::{AuthModal, AuthState};
pub use components::*;
pub use desktop::*;
pub use desktop_window::*;
pub use interop::*;
pub use terminal::*;
pub use viewers::*;

use dioxus::prelude::*;

#[component]
fn App() -> Element {
    rsx! {
        Desktop { desktop_id: "default-desktop".to_string() }
    }
}

/// WASM entry point — called by wasm-bindgen when the module is loaded.
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_main() {
    wasm_logger::init(wasm_logger::Config::default());
    dioxus_logger::init(dioxus_logger::tracing::Level::INFO).ok();
    dioxus::launch(App);
}
