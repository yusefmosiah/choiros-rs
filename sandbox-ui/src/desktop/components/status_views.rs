use dioxus::prelude::*;

#[component]
pub fn LoadingState() -> Element {
    rsx! {
        div {
            style: "display: flex; align-items: center; justify-content: center; height: 100%; color: var(--text-muted, #6b7280);",
            "Loading desktop..."
        }
    }
}

#[component]
pub fn ErrorState(error: String) -> Element {
    rsx! {
        div {
            style: "display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--danger-text, #ef4444); padding: 2rem; text-align: center;",
            p { style: "font-weight: 500; margin-bottom: 0.5rem;", "Error loading desktop" }
            p { style: "font-size: 0.875rem; color: var(--text-secondary, #9ca3af);", "{error}" }
        }
    }
}
