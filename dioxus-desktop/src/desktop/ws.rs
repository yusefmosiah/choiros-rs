use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use shared_types::{
    ChangesetImpact, DesktopState, FailureKind, PatchOp, PatchSource, WindowState,
    WriterRunEventBase, WriterRunPatchPayload, WriterRunStatusKind,
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
    WindowFocused(String),
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
    let json = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    let msg_type = json.get("type")?.as_str()?;

    match msg_type {
        "pong" => Some(WsEvent::Pong),
        "desktop_state" => {
            serde_json::from_value::<DesktopState>(json.get("desktop").cloned().unwrap_or_default())
                .ok()
                .map(WsEvent::DesktopStateUpdate)
        }
        "window_opened" => {
            serde_json::from_value::<WindowState>(json.get("window").cloned().unwrap_or_default())
                .ok()
                .map(WsEvent::WindowOpened)
        }
        "window_closed" => json
            .get("window_id")
            .and_then(|v| v.as_str())
            .map(|window_id| WsEvent::WindowClosed(window_id.to_string())),
        "window_moved" => {
            if let (Some(window_id), Some(x), Some(y)) = (
                json.get("window_id").and_then(|v| v.as_str()),
                json.get("x").and_then(|v| v.as_i64()),
                json.get("y").and_then(|v| v.as_i64()),
            ) {
                Some(WsEvent::WindowMoved {
                    window_id: window_id.to_string(),
                    x: x as i32,
                    y: y as i32,
                })
            } else {
                None
            }
        }
        "window_resized" => {
            if let (Some(window_id), Some(width), Some(height)) = (
                json.get("window_id").and_then(|v| v.as_str()),
                json.get("width").and_then(|v| v.as_i64()),
                json.get("height").and_then(|v| v.as_i64()),
            ) {
                Some(WsEvent::WindowResized {
                    window_id: window_id.to_string(),
                    width: width as i32,
                    height: height as i32,
                })
            } else {
                None
            }
        }
        "window_focused" => json
            .get("window_id")
            .and_then(|v| v.as_str())
            .map(|window_id| WsEvent::WindowFocused(window_id.to_string())),
        "window_minimized" => json
            .get("window_id")
            .and_then(|v| v.as_str())
            .map(|window_id| WsEvent::WindowMinimized(window_id.to_string())),
        "window_maximized" => {
            if let (Some(window_id), Some(x), Some(y), Some(width), Some(height)) = (
                json.get("window_id").and_then(|v| v.as_str()),
                json.get("x").and_then(|v| v.as_i64()),
                json.get("y").and_then(|v| v.as_i64()),
                json.get("width").and_then(|v| v.as_i64()),
                json.get("height").and_then(|v| v.as_i64()),
            ) {
                Some(WsEvent::WindowMaximized {
                    window_id: window_id.to_string(),
                    x: x as i32,
                    y: y as i32,
                    width: width as i32,
                    height: height as i32,
                })
            } else {
                None
            }
        }
        "window_restored" => {
            if let (
                Some(window_id),
                Some(x),
                Some(y),
                Some(width),
                Some(height),
                Some(_from),
                Some(maximized),
            ) = (
                json.get("window_id").and_then(|v| v.as_str()),
                json.get("x").and_then(|v| v.as_i64()),
                json.get("y").and_then(|v| v.as_i64()),
                json.get("width").and_then(|v| v.as_i64()),
                json.get("height").and_then(|v| v.as_i64()),
                json.get("from").and_then(|v| v.as_str()),
                json.get("maximized").and_then(|v| v.as_bool()),
            ) {
                Some(WsEvent::WindowRestored {
                    window_id: window_id.to_string(),
                    x: x as i32,
                    y: y as i32,
                    width: width as i32,
                    height: height as i32,
                    maximized,
                })
            } else {
                None
            }
        }
        "telemetry" => {
            if let (Some(event_type), Some(capability), Some(phase), Some(importance)) = (
                json.get("event_type").and_then(|v| v.as_str()),
                json.get("capability").and_then(|v| v.as_str()),
                json.get("phase").and_then(|v| v.as_str()),
                json.get("importance").and_then(|v| v.as_str()),
            ) {
                let data = json.get("data").cloned().unwrap_or_default();
                Some(WsEvent::Telemetry {
                    event_type: event_type.to_string(),
                    capability: capability.to_string(),
                    phase: phase.to_string(),
                    importance: importance.to_string(),
                    data,
                })
            } else {
                None
            }
        }
        "conductor.run.document_update" => {
            if let (Some(run_id), Some(document_path), Some(content_excerpt)) = (
                json.get("run_id").and_then(|v| v.as_str()),
                json.get("document_path").and_then(|v| v.as_str()),
                json.get("content_excerpt").and_then(|v| v.as_str()),
            ) {
                Some(WsEvent::DocumentUpdate {
                    run_id: run_id.to_string(),
                    document_path: document_path.to_string(),
                    content_excerpt: content_excerpt.to_string(),
                    timestamp: json
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
            } else {
                None
            }
        }
        "writer.run.started" => parse_writer_run_started(&json),
        "writer.run.progress" => parse_writer_run_progress(&json),
        "writer.run.patch" => parse_writer_run_patch(&json),
        "writer.run.changeset" => parse_writer_run_changeset(&json),
        "writer.run.status" => parse_writer_run_status(&json),
        "writer.run.failed" => parse_writer_run_failed(&json),
        "error" => json
            .get("message")
            .and_then(|v| v.as_str())
            .map(|message| WsEvent::Error(message.to_string())),
        _ => None,
    }
}

fn parse_writer_run_base(json: &serde_json::Value) -> Option<WriterRunEventBase> {
    Some(WriterRunEventBase {
        desktop_id: json.get("desktop_id")?.as_str()?.to_string(),
        session_id: json.get("session_id")?.as_str()?.to_string(),
        thread_id: json.get("thread_id")?.as_str()?.to_string(),
        run_id: json.get("run_id")?.as_str()?.to_string(),
        document_path: json.get("document_path")?.as_str()?.to_string(),
        revision: json.get("revision")?.as_u64()?,
        timestamp: json
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(chrono::Utc::now),
    })
}

fn parse_writer_run_started(json: &serde_json::Value) -> Option<WsEvent> {
    let base = parse_writer_run_base(json)?;
    let objective = json.get("objective")?.as_str()?.to_string();
    Some(WsEvent::WriterRunStarted { base, objective })
}

fn parse_writer_run_progress(json: &serde_json::Value) -> Option<WsEvent> {
    let base = parse_writer_run_base(json)?;
    let phase = json.get("phase")?.as_str()?.to_string();
    let message = json.get("message")?.as_str()?.to_string();
    let progress_pct = json
        .get("progress_pct")
        .and_then(|v| v.as_u64())
        .map(|v| v as u8);
    Some(WsEvent::WriterRunProgress {
        base,
        phase,
        message,
        progress_pct,
    })
}

fn parse_patch_ops(json: &serde_json::Value) -> Option<Vec<PatchOp>> {
    let ops_array = json.get("ops")?.as_array()?;
    let mut ops = Vec::new();
    for op_json in ops_array {
        let op_type = op_json.get("op")?.as_str()?;
        let patch_op = match op_type {
            "insert" => PatchOp::Insert {
                pos: op_json.get("pos")?.as_u64()?,
                text: op_json.get("text")?.as_str()?.to_string(),
            },
            "delete" => PatchOp::Delete {
                pos: op_json.get("pos")?.as_u64()?,
                len: op_json.get("len")?.as_u64()?,
            },
            "replace" => PatchOp::Replace {
                pos: op_json.get("pos")?.as_u64()?,
                len: op_json.get("len")?.as_u64()?,
                text: op_json.get("text")?.as_str()?.to_string(),
            },
            "retain" => PatchOp::Retain {
                len: op_json.get("len")?.as_u64()?,
            },
            _ => continue,
        };
        ops.push(patch_op);
    }
    Some(ops)
}

fn parse_patch_source(json: &serde_json::Value) -> Option<PatchSource> {
    match json.get("source")?.as_str()? {
        "agent" => Some(PatchSource::Agent),
        "user" => Some(PatchSource::User),
        "system" => Some(PatchSource::System),
        _ => None,
    }
}

fn parse_writer_run_patch(json: &serde_json::Value) -> Option<WsEvent> {
    let base = parse_writer_run_base(json)?;
    let ops = parse_patch_ops(json)?;
    let payload = WriterRunPatchPayload {
        patch_id: json.get("patch_id")?.as_str()?.to_string(),
        source: parse_patch_source(json)?,
        section_id: json
            .get("section_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        ops,
        proposal: json
            .get("proposal")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        base_version_id: json.get("base_version_id").and_then(|v| v.as_u64()),
        target_version_id: json.get("target_version_id").and_then(|v| v.as_u64()),
        overlay_id: json
            .get("overlay_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };
    Some(WsEvent::WriterRunPatch { base, payload })
}

fn parse_writer_run_status(json: &serde_json::Value) -> Option<WsEvent> {
    let base = parse_writer_run_base(json)?;
    let status_str = json.get("status")?.as_str()?;
    let status = match status_str {
        "initializing" => WriterRunStatusKind::Initializing,
        "running" => WriterRunStatusKind::Running,
        "waiting_for_worker" => WriterRunStatusKind::WaitingForWorker,
        "completing" => WriterRunStatusKind::Completing,
        "completed" => WriterRunStatusKind::Completed,
        "failed" => WriterRunStatusKind::Failed,
        "blocked" => WriterRunStatusKind::Blocked,
        _ => return None,
    };
    let message = json
        .get("message")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    Some(WsEvent::WriterRunStatus {
        base,
        status,
        message,
    })
}

fn parse_failure_kind(json: &serde_json::Value) -> Option<FailureKind> {
    match json.get("failure_kind")?.as_str()? {
        "timeout" => Some(FailureKind::Timeout),
        "network" => Some(FailureKind::Network),
        "auth" => Some(FailureKind::Auth),
        "rate_limit" => Some(FailureKind::RateLimit),
        "validation" => Some(FailureKind::Validation),
        "provider" => Some(FailureKind::Provider),
        _ => Some(FailureKind::Unknown),
    }
}

fn parse_writer_run_failed(json: &serde_json::Value) -> Option<WsEvent> {
    let base = parse_writer_run_base(json)?;
    let error_code = json.get("error_code")?.as_str()?.to_string();
    let error_message = json.get("error_message")?.as_str()?.to_string();
    let failure_kind = parse_failure_kind(json);
    Some(WsEvent::WriterRunFailed {
        base,
        error_code,
        error_message,
        failure_kind,
    })
}

fn parse_writer_run_changeset(json: &serde_json::Value) -> Option<WsEvent> {
    let base = parse_writer_run_base(json)?;
    let patch_id = json.get("patch_id")?.as_str()?.to_string();
    let loop_id = json
        .get("loop_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let summary = json.get("summary")?.as_str()?.to_string();
    let impact = match json.get("impact").and_then(|v| v.as_str()).unwrap_or("low") {
        "high" => ChangesetImpact::High,
        "medium" => ChangesetImpact::Medium,
        _ => ChangesetImpact::Low,
    };
    let op_taxonomy = json
        .get("op_taxonomy")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    Some(WsEvent::WriterRunChangeset {
        base,
        patch_id,
        loop_id,
        summary,
        impact,
        op_taxonomy,
    })
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

        let subscribe_msg =
            format!("{{\"type\":\"subscribe\",\"desktop_id\":\"{desktop_id_clone}\"}}");
        let _ = ws_clone.send_with_str(&subscribe_msg);
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
