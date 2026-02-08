use dioxus::prelude::*;

#[component]
pub fn MarkdownViewer(html: String) -> Element {
    rsx! {
        div {
            style: "height: 100%; overflow: auto; background: #0b1220; color: #e2e8f0; padding: 16px; line-height: 1.6;",
            article {
                style: "max-width: 900px; margin: 0 auto;",
                dangerous_inner_html: "{html}"
            }
        }
    }
}
