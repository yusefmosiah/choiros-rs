use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_namespace = window)]
extern "C" {
    #[wasm_bindgen(js_name = createTextViewer)]
    fn create_text_viewer(container: web_sys::Element, options: &JsValue) -> u32;

    #[wasm_bindgen(js_name = setTextViewerContent)]
    fn set_text_viewer_content(handle: u32, text: &str);

    #[wasm_bindgen(js_name = onTextViewerChange)]
    fn on_text_viewer_change(handle: u32, cb: &Closure<dyn FnMut(String)>);

    #[wasm_bindgen(js_name = disposeTextViewer)]
    fn dispose_text_viewer(handle: u32);
}

struct TextViewerRuntime {
    handle: u32,
    _on_change: Closure<dyn FnMut(String)>,
}

impl Drop for TextViewerRuntime {
    fn drop(&mut self) {
        dispose_text_viewer(self.handle);
    }
}

#[component]
pub fn TextViewer(content: String, readonly: bool, on_change: Callback<String>) -> Element {
    let container_id = use_signal(|| format!("text-viewer-{}", uuid::Uuid::new_v4()));
    let mut runtime = use_signal(|| None::<TextViewerRuntime>);
    let cid = container_id();

    let content_for_mount = content.clone();
    let container_id_for_mount = container_id();
    use_effect(move || {
        if runtime.read().is_some() {
            return;
        }

        let content = content_for_mount.clone();
        let container_id = container_id_for_mount.clone();
        spawn(async move {
            if let Err(e) = ensure_text_viewer_script().await {
                dioxus_logger::tracing::error!("failed to load text viewer script: {:?}", e);
                return;
            }
            if !wait_for_text_viewer_bridge().await {
                dioxus_logger::tracing::error!("text viewer bridge is unavailable");
                return;
            }

            let Some(container) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id(&container_id))
            else {
                return;
            };

            let options = serde_json::json!({
                "initialContent": content,
                "readOnly": readonly,
            });
            let options_js = JsValue::from_str(&options.to_string());
            let handle = create_text_viewer(container, &options_js);
            if handle == 0 {
                dioxus_logger::tracing::error!("failed to create text viewer handle");
                return;
            }

            let change_signal = on_change;
            let on_change_cb = Closure::wrap(Box::new(move |next: String| {
                change_signal.call(next);
            }) as Box<dyn FnMut(String)>);
            on_text_viewer_change(handle, &on_change_cb);

            runtime.set(Some(TextViewerRuntime {
                handle,
                _on_change: on_change_cb,
            }));
        });
    });

    let content_for_sync = content.clone();
    use_effect(move || {
        if let Some(rt) = runtime.read().as_ref() {
            set_text_viewer_content(rt.handle, &content_for_sync);
        }
    });

    rsx! {
        div {
            id: "{cid}",
            class: "text-viewer-host",
            style: "height: 100%; width: 100%;",
        }
    }
}

async fn ensure_text_viewer_script() -> Result<(), JsValue> {
    ensure_script("viewer-text-bridge-js", "/viewer-text.js")
}

fn ensure_script(id: &str, src: &str) -> Result<(), JsValue> {
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| JsValue::from_str("document unavailable"))?;

    if document.get_element_by_id(id).is_some() {
        return Ok(());
    }

    let script: web_sys::HtmlScriptElement = document
        .create_element("script")?
        .dyn_into::<web_sys::HtmlScriptElement>()?;
    script.set_id(id);
    script.set_src(src);
    script.set_async(false);

    if let Some(head) = document.head() {
        head.append_child(&script)?;
    } else if let Some(body) = document.body() {
        body.append_child(&script)?;
    }

    Ok(())
}

async fn wait_for_text_viewer_bridge() -> bool {
    for _ in 0..30 {
        if has_text_viewer_bridge() {
            return true;
        }
        TimeoutFuture::new(100).await;
    }
    false
}

fn has_text_viewer_bridge() -> bool {
    let global = js_sys::global();
    js_sys::Reflect::has(&global, &JsValue::from_str("createTextViewer")).unwrap_or(false)
}
