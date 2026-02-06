use dioxus::launch;
use dioxus::prelude::*;
use dioxus_logger::tracing::Level;

use sandbox_ui::Desktop;

fn main() {
    // Initialize logging for WASM
    wasm_logger::init(wasm_logger::Config::default());
    dioxus_logger::init(Level::INFO).ok();

    launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        Desktop { desktop_id: "default-desktop".to_string() }
    }
}
