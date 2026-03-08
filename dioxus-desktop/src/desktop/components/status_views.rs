use dioxus::prelude::*;

#[component]
pub fn LoadingState() -> Element {
    rsx! {
        div {
            style: "
                display: flex; flex-direction: column; align-items: flex-start;
                justify-content: center; height: 100%;
                font-family: 'IBM Plex Mono', 'Menlo', 'Consolas', monospace;
                font-size: 13px; line-height: 1.55;
                color: #c0c8d8; padding: 2rem 3rem;
                background: #0a0e1a;
            ",
            div { style: "color: #7aa2f7; font-weight: bold; margin-bottom: 0.75rem;",
                "Initializing Desktop Environment"
            }
            div { style: "color: #565f89;",
                "Loading workspace state . . . . . . "
                span { style: "color: #e0af68;", "[  ..  ]" }
            }
            div { style: "color: #565f89; margin-top: 0.25rem;",
                "Connecting event bus . . . . . . . . "
                span { style: "color: #e0af68;", "[  ..  ]" }
            }
        }
    }
}

#[component]
pub fn ErrorState(error: String) -> Element {
    rsx! {
        div {
            style: "
                display: flex; flex-direction: column; align-items: flex-start;
                justify-content: center; height: 100%;
                font-family: 'IBM Plex Mono', 'Menlo', 'Consolas', monospace;
                font-size: 13px; line-height: 1.55;
                color: #c0c8d8; padding: 2rem 3rem;
                background: #0a0e1a;
            ",
            div { style: "color: #f7768e; font-weight: bold; margin-bottom: 0.75rem;",
                "System Error"
            }
            div { style: "color: #565f89;",
                "Desktop initialization . . . . . .  "
                span { style: "color: #f7768e;", "[ FAIL ]" }
            }
            div { style: "color: #f7768e; margin-top: 0.5rem; font-size: 12px;",
                "{error}"
            }
        }
    }
}
