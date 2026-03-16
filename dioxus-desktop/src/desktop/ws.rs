use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use shared_types::{
    AppDefinition, ChangesetImpact, DesktopState, DesktopWsMessage, EventImportance,
    FailureKind, WindowState, WriterRunEventBase, WriterRunPatchPayload, WriterRunStatusKind,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

pub struct DesktopWsRuntime {
    ws: WebSocket,
    closing: Rc<Cell<bool>>,
    _on_open: Closure<dyn FnMut(wasm_bindgen::JsValue)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_close: Closure<dyn FnMut(wasm_bindgen::JsValue)>,
    _on_error: Closure<dyn FnMut(wasm_bindgen::JsValue)>,
}

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected,
    Disconnected,
    DesktopStateUpdate(DesktopState),
    AppRegistered(AppDefinition),
    WindowOpened(WindowState),
    WindowClosed(String),
    WindowMoved {
        window_id: String,
        x: i32,
        y: i32,
    },
    WindowResized {
        window_id: String,
        width: i32,
        height: i32,
    },
    WindowFocused {
        window_id: String,
        z_index: u32,
    },
    WindowMinimized(String),
    WindowMaximized {
        window_id: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    WindowRestored {
        window_id: String,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        maximized: bool,
    },
    /// Telemetry event for live stream display
    Telemetry {
        event_type: String,
        capability: String,
        phase: String,
        importance: String,
        data: serde_json::Value,
    },
    /// Document update for live streaming of conductor run documents
    DocumentUpdate {
        run_id: String,
        document_path: String,
        content_excerpt: String,
        timestamp: String,
    },
    /// Writer run started event
    WriterRunStarted {
        base: WriterRunEventBase,
        objective: String,
    },
    /// Writer run progress event
    WriterRunProgress {
        base: WriterRunEventBase,
        phase: String,
        message: String,
        progress_pct: Option<u8>,
        source_refs: Vec<String>,
    },
    /// Writer run patch event for live document updates
    WriterRunPatch {
        base: WriterRunEventBase,
        payload: WriterRunPatchPayload,
    },
    /// Writer run status change event
    WriterRunStatus {
        base: WriterRunEventBase,
        status: WriterRunStatusKind,
        message: Option<String>,
    },
    /// Writer run failed event
    WriterRunFailed {
        base: WriterRunEventBase,
        error_code: String,
        error_message: String,
        failure_kind: Option<FailureKind>,
    },
    /// Semantic changeset summary for Marginalia observation UX
    WriterRunChangeset {
        base: WriterRunEventBase,
        patch_id: String,
        loop_id: Option<String>,
        summary: String,
        impact: ChangesetImpact,
        op_taxonomy: Vec<String>,
    },
    Pong,
    Error(String),
}

pub fn http_to_ws_url(http_url: &str) -> String {
    if http_url.starts_with("http://") {
        http_url.replace("http://", "ws://")
    } else if http_url.starts_with("https://") {
        http_url.replace("https://", "wss://")
    } else if http_url.is_empty() {
        let protocol = web_sys::window()
            .and_then(|w| w.location().protocol().ok())
            .unwrap_or_else(|| "http:".to_string());
        let host = web_sys::window()
            .and_then(|w| w.location().host().ok())
            .unwrap_or_else(|| "localhost".to_string());

        if protocol == "https:" {
            format!("wss://{host}")
        } else {
            format!("ws://{host}")
        }
    } else {
        format!("ws://{http_url}")
    }
}

pub fn parse_ws_message(payload: &str) -> Option<WsEvent> {
    let message: DesktopWsMessage = serde_json::from_str(payload).ok()?;

    match message {
        DesktopWsMessage::Pong => Some(WsEvent::Pong),
        DesktopWsMessage::DesktopState { desktop } => Some(WsEvent::DesktopStateUpdate(desktop)),
        DesktopWsMessage::AppRegistered { app } => Some(WsEvent::AppRegistered(app)),
        DesktopWsMessage::WindowOpened { window } => Some(WsEvent::WindowOpened(window)),
        DesktopWsMessage::WindowClosed { window_id } => Some(WsEvent::WindowClosed(window_id)),
        DesktopWsMessage::WindowMoved { window_id, x, y } => {
            Some(WsEvent::WindowMoved { window_id, x, y })
        }
        DesktopWsMessage::WindowResized {
            window_id,
            width,
            height,
        } => Some(WsEvent::WindowResized {
            window_id,
            width,
            height,
        }),
        DesktopWsMessage::WindowFocused { window_id, z_index } => {
            Some(WsEvent::WindowFocused { window_id, z_index })
        }
        DesktopWsMessage::WindowMinimized { window_id } => Some(WsEvent::WindowMinimized(window_id)),
        DesktopWsMessage::WindowMaximized {
            window_id,
            x,
            y,
            width,
            height,
        } => Some(WsEvent::WindowMaximized {
            window_id,
            x,
            y,
            width,
            height,
        }),
        DesktopWsMessage::WindowRestored {
            window_id,
            x,
            y,
            width,
            height,
            from: _,
            maximized,
        } => Some(WsEvent::WindowRestored {
            window_id,
            x,
            y,
            width,
            height,
            maximized,
        }),
        DesktopWsMessage::Telemetry { payload } => Some(WsEvent::Telemetry {
            event_type: payload.event_type,
            capability: payload.capability,
            phase: payload.phase,
            importance: importance_to_string(payload.importance),
            data: payload.data,
        }),
        DesktopWsMessage::DocumentUpdate { payload } => Some(WsEvent::DocumentUpdate {
            run_id: payload.run_id,
            document_path: payload.document_path,
            content_excerpt: payload.content_excerpt,
            timestamp: payload.timestamp,
        }),
        DesktopWsMessage::WriterRunStarted { base, objective } => {
            Some(WsEvent::WriterRunStarted { base, objective })
        }
        DesktopWsMessage::WriterRunProgress {
            base,
            phase,
            message,
            progress_pct,
            source_refs,
        } => Some(WsEvent::WriterRunProgress {
            base,
            phase,
            message,
            progress_pct,
            source_refs,
        }),
        DesktopWsMessage::WriterRunPatch { base, payload } => {
            Some(WsEvent::WriterRunPatch { base, payload })
        }
        DesktopWsMessage::WriterRunStatus {
            base,
            status,
            message,
        } => Some(WsEvent::WriterRunStatus {
            base,
            status,
            message,
        }),
        DesktopWsMessage::WriterRunFailed {
            base,
            error_code,
            error_message,
            failure_kind,
        } => Some(WsEvent::WriterRunFailed {
            base,
            error_code,
            error_message,
            failure_kind,
        }),
        DesktopWsMessage::WriterRunChangeset { base, payload } => {
            Some(WsEvent::WriterRunChangeset {
                base,
                patch_id: payload.patch_id,
                loop_id: payload.loop_id,
                summary: payload.summary,
                impact: payload.impact,
                op_taxonomy: payload.op_taxonomy,
            })
        }
        DesktopWsMessage::Error { message } => Some(WsEvent::Error(message)),
        DesktopWsMessage::Subscribe { .. } | DesktopWsMessage::Ping => None,
    }
}

fn importance_to_string(importance: EventImportance) -> String {
    match importance {
        EventImportance::High => "high".to_string(),
        EventImportance::Normal => "normal".to_string(),
        EventImportance::Low => "low".to_string(),
    }
}

pub fn connect_websocket<F>(desktop_id: &str, on_event: F) -> Result<DesktopWsRuntime, String>
where
    F: FnMut(WsEvent) + 'static,
{
    let api_base = crate::api::api_base();
    let ws_base = http_to_ws_url(api_base);
    let ws_url = format!("{ws_base}/ws");

    dioxus_logger::tracing::info!("Connecting to WebSocket: {}", ws_url);

    let ws = match WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(e) => {
            dioxus_logger::tracing::error!("Failed to create WebSocket: {:?}", e);
            return Err(format!("Failed to create websocket: {e:?}"));
        }
    };

    let closing = Rc::new(Cell::new(false));
    let on_event_rc = Rc::new(RefCell::new(on_event));
    let on_event_open = on_event_rc.clone();
    let on_event_close = on_event_rc.clone();
    let closing_for_close = closing.clone();
    let desktop_id_clone = desktop_id.to_string();
    let ws_clone = ws.clone();

    let onopen_callback = Closure::wrap(Box::new(move |_e: wasm_bindgen::JsValue| {
        dioxus_logger::tracing::info!("WebSocket connected");
        on_event_open.borrow_mut()(WsEvent::Connected);

        if let Ok(subscribe_msg) =
            serde_json::to_string(&DesktopWsMessage::Subscribe { desktop_id: desktop_id_clone.clone() })
        {
            let _ = ws_clone.send_with_str(&subscribe_msg);
        }
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onopen(Some(onopen_callback.as_ref().unchecked_ref()));

    let onmessage_callback = Closure::wrap(Box::new(move |e: MessageEvent| {
        if let Ok(text) = e.data().dyn_into::<js_sys::JsString>() {
            let text_str = text.as_string().unwrap_or_default();
            dioxus_logger::tracing::debug!("WebSocket message: {}", text_str);

            if let Some(event) = parse_ws_message(&text_str) {
                match &event {
                    WsEvent::Pong => {
                        dioxus_logger::tracing::debug!("WebSocket pong received");
                    }
                    WsEvent::Error(message) => {
                        dioxus_logger::tracing::error!("WebSocket error message: {}", message);
                    }
                    _ => {}
                }
                on_event_rc.borrow_mut()(event);
            }
        }
    }) as Box<dyn FnMut(MessageEvent)>);
    ws.set_onmessage(Some(onmessage_callback.as_ref().unchecked_ref()));

    let onclose_callback = Closure::wrap(Box::new(move |_e: wasm_bindgen::JsValue| {
        if closing_for_close.get() {
            return;
        }
        dioxus_logger::tracing::info!("WebSocket disconnected");
        on_event_close.borrow_mut()(WsEvent::Disconnected);
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onclose(Some(onclose_callback.as_ref().unchecked_ref()));

    let onerror_callback = Closure::wrap(Box::new(move |e: wasm_bindgen::JsValue| {
        dioxus_logger::tracing::error!("WebSocket error: {:?}", e);
    }) as Box<dyn FnMut(wasm_bindgen::JsValue)>);
    ws.set_onerror(Some(onerror_callback.as_ref().unchecked_ref()));

    Ok(DesktopWsRuntime {
        ws,
        closing,
        _on_open: onopen_callback,
        _on_message: onmessage_callback,
        _on_close: onclose_callback,
        _on_error: onerror_callback,
    })
}

impl Drop for DesktopWsRuntime {
    fn drop(&mut self) {
        self.closing.set(true);
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_ws_message, WsEvent};

    #[test]
    fn parse_app_registered_message() {
        let payload = serde_json::json!({
            "type": "app_registered",
            "app": {
                "id": "trace",
                "name": "Trace",
                "icon": "🔍",
                "component_code": "TraceApp",
                "default_width": 900,
                "default_height": 600
            }
        });

        let event =
            parse_ws_message(&payload.to_string()).expect("expected parsed websocket event");

        match event {
            WsEvent::AppRegistered(app) => {
                assert_eq!(app.id, "trace");
                assert_eq!(app.name, "Trace");
            }
            other => panic!("unexpected websocket event: {other:?}"),
        }
    }

    #[test]
    fn parse_changeset_message_uses_full_writer_run_base_when_present() {
        let payload = serde_json::json!({
            "type": "writer.run.changeset",
            "desktop_id": "desktop-1",
            "session_id": "session-1",
            "thread_id": "thread-1",
            "run_id": "run-1",
            "document_path": "conductor/runs/run-1/draft.md",
            "revision": 5,
            "timestamp": "2026-03-13T22:00:00Z",
            "patch_id": "patch-1",
            "loop_id": "loop-1",
            "target_version_id": 2,
            "source": "writer",
            "summary": "Reworked the introduction.",
            "impact": "high",
            "op_taxonomy": ["replace", "structural_rewrite"]
        });

        let event =
            parse_ws_message(&payload.to_string()).expect("expected parsed websocket event");

        match event {
            WsEvent::WriterRunChangeset {
                base,
                patch_id,
                loop_id,
                summary,
                impact,
                op_taxonomy,
            } => {
                assert_eq!(base.desktop_id, "desktop-1");
                assert_eq!(base.session_id, "session-1");
                assert_eq!(base.thread_id, "thread-1");
                assert_eq!(base.run_id, "run-1");
                assert_eq!(base.document_path, "conductor/runs/run-1/draft.md");
                assert_eq!(base.revision, 5);
                assert_eq!(patch_id, "patch-1");
                assert_eq!(loop_id.as_deref(), Some("loop-1"));
                assert_eq!(summary, "Reworked the introduction.");
                assert_eq!(impact, shared_types::ChangesetImpact::High);
                assert_eq!(
                    op_taxonomy,
                    vec!["replace".to_string(), "structural_rewrite".to_string()]
                );
            }
            other => panic!("unexpected websocket event: {other:?}"),
        }
    }
}
