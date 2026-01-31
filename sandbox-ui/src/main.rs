use dioxus::launch;
use dioxus::prelude::*;
use dioxus_logger::tracing::Level;

use sandbox_ui::ChatView;

fn main() {
    // Initialize logging for WASM
    wasm_logger::init(wasm_logger::Config::default());
    dioxus_logger::init(Level::INFO).ok();

    launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        div {
            style: "min-height: 100vh; background-color: #111827; color: white; padding: 1rem;",
            ChatView { actor_id: "default-chat".to_string() }
        }
    }
}
