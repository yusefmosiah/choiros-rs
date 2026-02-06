use dioxus::prelude::*;

#[component]
pub fn ImageViewer(content: String, fallback_uri: String) -> Element {
    let mut scale = use_signal(|| 1.0f64);
    let mut offset_x = use_signal(|| 0.0f64);
    let mut offset_y = use_signal(|| 0.0f64);
    let mut dragging = use_signal(|| false);
    let mut drag_origin = use_signal(|| (0.0f64, 0.0f64));

    let src = if content.starts_with("data:") {
        content
    } else {
        fallback_uri
    };

    let on_reset = move |_| {
        scale.set(1.0);
        offset_x.set(0.0);
        offset_y.set(0.0);
    };

    rsx! {
        div {
            style: "height: 100%; display: flex; flex-direction: column; background: #0b1220;",
            div {
                style: "display: flex; gap: 8px; padding: 8px; border-bottom: 1px solid #1f2937;",
                button {
                    onclick: move |_| scale.set((scale() + 0.1).min(4.0)),
                    "+"
                }
                button {
                    onclick: move |_| scale.set((scale() - 0.1).max(0.2)),
                    "-"
                }
                button { onclick: on_reset, "Reset" }
            }
            div {
                style: "flex: 1; overflow: hidden; position: relative; cursor: grab;",
                onmousedown: move |evt| {
                    dragging.set(true);
                    drag_origin.set((evt.client_coordinates().x as f64, evt.client_coordinates().y as f64));
                },
                onmouseup: move |_| dragging.set(false),
                onmouseleave: move |_| dragging.set(false),
                onmousemove: move |evt| {
                    if !dragging() {
                        return;
                    }
                    let (start_x, start_y) = drag_origin();
                    let now_x = evt.client_coordinates().x as f64;
                    let now_y = evt.client_coordinates().y as f64;
                    offset_x.set(offset_x() + (now_x - start_x));
                    offset_y.set(offset_y() + (now_y - start_y));
                    drag_origin.set((now_x, now_y));
                },
                img {
                    src: "{src}",
                    style: "max-width: none; max-height: none; user-select: none; transform: translate({offset_x}px, {offset_y}px) scale({scale}); transform-origin: center center; position: absolute; top: 50%; left: 50%;",
                }
            }
        }
    }
}
