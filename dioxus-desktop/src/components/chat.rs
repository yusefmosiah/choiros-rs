use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use shared_types::{ChatMessage, Sender};
use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{CloseEvent, ErrorEvent, Event, MessageEvent, WebSocket};

use super::styles::CHAT_STYLES;
use crate::api::{fetch_messages, fetch_windows, focus_window, send_chat_message};

const TOOL_CALL_PREFIX: &str = "__tool_call__:";
const TOOL_RESULT_PREFIX: &str = "__tool_result__:";
const ACTOR_CALL_PREFIX: &str = "__actor_call__:";
const ASSISTANT_BUNDLE_PREFIX: &str = "__assistant_bundle__:";

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct ToolEntry {
    kind: String,
    payload: serde_json::Value,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct AssistantBundle {
    text: String,
    thinking: Vec<String>,
    tools: Vec<ToolEntry>,
    #[serde(default)]
    model_used: Option<String>,
    #[serde(default)]
    model_source: Option<String>,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct ChatResponsePayload {
    text: String,
    #[serde(default)]
    model_used: Option<String>,
    #[serde(default)]
    model_source: Option<String>,
}

enum ChatWsEvent {
    Connected,
    Message(String),
    Error(String),
    Closed,
}

struct ChatRuntime {
    ws: WebSocket,
    closing: Rc<Cell<bool>>,
    _on_open: Closure<dyn FnMut(Event)>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
    _on_error: Closure<dyn FnMut(ErrorEvent)>,
    _on_close: Closure<dyn FnMut(CloseEvent)>,
}

#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct ChatThreadEntry {
    thread_id: String,
    title: String,
    last_preview: String,
    last_updated_ms: i64,
}

#[component]
pub fn ChatView(actor_id: String, desktop_id: String, window_id: String) -> Element {
    let mut messages = use_signal(Vec::<ChatMessage>::new);
    let mut input_text = use_signal(String::new);
    let user_id = use_signal(|| "user-1".to_string());
    let mut loading = use_signal(|| false);
    let mut ws_runtime = use_signal(|| None::<ChatRuntime>);
    let mut ws_connected = use_signal(|| false);
    let ws_event_queue =
        use_hook(|| Rc::new(std::cell::RefCell::new(VecDeque::<ChatWsEvent>::new())));
    let mut ws_event_pump_started = use_signal(|| false);
    let ws_event_pump_alive = use_hook(|| Rc::new(Cell::new(true)));
    let mut actor_id_signal = use_signal(|| actor_id.clone());
    let mut thread_entries = use_signal(load_chat_thread_entries);
    let mut thread_sidebar_collapsed = use_signal(default_thread_sidebar_collapsed);
    let _messages_end_ref = use_signal(|| None::<dioxus::prelude::Element>);

    {
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_drop(move || {
            ws_event_pump_alive.set(false);
        });
    }

    // Ensure active thread exists in local thread history.
    use_effect(move || {
        let active_thread_id = actor_id_signal.to_string();
        let mut entries = load_chat_thread_entries();
        upsert_thread_entry(
            &mut entries,
            ChatThreadEntry {
                thread_id: active_thread_id.clone(),
                title: short_thread_title(&active_thread_id),
                last_preview: String::new(),
                last_updated_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
        persist_chat_thread_entries(&entries);
        thread_entries.set(entries);
    });

    // Reconnect websocket when active thread changes.
    use_effect(move || {
        let _ = actor_id_signal.to_string();
        if let Some(runtime) = ws_runtime.write().take() {
            runtime.closing.set(true);
            let _ = runtime.ws.close();
        }
        ws_connected.set(false);
    });

    // Load messages on mount
    use_effect(move || {
        let actor_id = actor_id_signal.to_string();
        spawn(async move {
            match fetch_messages(&actor_id).await {
                Ok(msgs) => {
                    let normalized = collapse_tool_messages(msgs);
                    let mut entries = thread_entries.read().clone();
                    let preview = latest_message_preview(&normalized);
                    upsert_thread_entry(
                        &mut entries,
                        ChatThreadEntry {
                            thread_id: actor_id.clone(),
                            title: thread_title_from_messages(&actor_id, &normalized),
                            last_preview: preview,
                            last_updated_ms: chrono::Utc::now().timestamp_millis(),
                        },
                    );
                    persist_chat_thread_entries(&entries);
                    thread_entries.set(entries);
                    messages.set(normalized);
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to fetch messages: {}", e);
                }
            }
        });
    });

    // Connect WebSocket for streaming responses
    {
        let ws_event_queue = ws_event_queue.clone();
        let ws_event_pump_alive = ws_event_pump_alive.clone();
        use_effect(move || {
            if ws_event_pump_started() {
                return;
            }
            ws_event_pump_started.set(true);

            let ws_event_queue = ws_event_queue.clone();
            let ws_event_pump_alive = ws_event_pump_alive.clone();
            spawn(async move {
                while ws_event_pump_alive.get() {
                    let mut drained = Vec::new();
                    {
                        let mut queue = ws_event_queue.borrow_mut();
                        while let Some(event) = queue.pop_front() {
                            drained.push(event);
                        }
                    }

                    for event in drained {
                        match event {
                            ChatWsEvent::Connected => {
                                ws_connected.set(true);
                            }
                            ChatWsEvent::Message(text_str) => {
                                let Ok(json) = serde_json::from_str::<serde_json::Value>(&text_str)
                                else {
                                    continue;
                                };

                                let msg_type =
                                    json.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                match msg_type {
                                    "connected" => {
                                        ws_connected.set(true);
                                    }
                                    "thinking" => {
                                        let content = json
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let mut list = messages.write();
                                        update_or_create_pending_assistant_bundle(
                                            &mut list,
                                            |bundle| {
                                                if !content.trim().is_empty() {
                                                    bundle.thinking.push(content.to_string());
                                                }
                                            },
                                        );
                                    }
                                    "tool_call" => {
                                        let content = json
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        if let Ok(payload) =
                                            serde_json::from_str::<serde_json::Value>(content)
                                        {
                                            let mut list = messages.write();
                                            update_or_create_pending_assistant_bundle(
                                                &mut list,
                                                |bundle| {
                                                    bundle.tools.push(ToolEntry {
                                                        kind: "call".to_string(),
                                                        payload,
                                                    });
                                                },
                                            );
                                        }
                                    }
                                    "tool_result" => {
                                        let content = json
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        if let Ok(payload) =
                                            serde_json::from_str::<serde_json::Value>(content)
                                        {
                                            let mut list = messages.write();
                                            update_or_create_pending_assistant_bundle(
                                                &mut list,
                                                |bundle| {
                                                    bundle.tools.push(ToolEntry {
                                                        kind: "result".to_string(),
                                                        payload,
                                                    });
                                                },
                                            );
                                        }
                                    }
                                    "actor_call" => {
                                        let content = json
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        if let Ok(payload) =
                                            serde_json::from_str::<serde_json::Value>(content)
                                        {
                                            let mut list = messages.write();
                                            update_or_create_pending_assistant_bundle(
                                                &mut list,
                                                |bundle| {
                                                    bundle.tools.push(ToolEntry {
                                                        kind: "actor_call".to_string(),
                                                        payload,
                                                    });
                                                },
                                            );
                                        }
                                    }
                                    "response" => {
                                        let content = json
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let response_payload = parse_chat_response_payload(content);

                                        let mut list = messages.write();
                                        clear_pending_user_message(&mut list);
                                        update_or_create_pending_assistant_bundle(
                                            &mut list,
                                            |bundle| {
                                                bundle.text = response_payload.text.clone();
                                                bundle.model_used =
                                                    response_payload.model_used.clone();
                                                bundle.model_source =
                                                    response_payload.model_source.clone();
                                            },
                                        );
                                        mark_last_pending_assistant_complete(&mut list);
                                        loading.set(false);
                                    }
                                    "error" => {
                                        let message = json
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .or_else(|| {
                                                json.get("message").and_then(|v| v.as_str())
                                            })
                                            .unwrap_or("Chat error");

                                        let mut list = messages.write();
                                        clear_pending_user_message(&mut list);
                                        if has_pending_assistant_bundle(&list) {
                                            update_or_create_pending_assistant_bundle(
                                                &mut list,
                                                |bundle| {
                                                    bundle.text = message.to_string();
                                                },
                                            );
                                            mark_last_pending_assistant_complete(&mut list);
                                        } else {
                                            list.push(ChatMessage {
                                                id: format!(
                                                    "error-{}",
                                                    chrono::Utc::now().timestamp_millis()
                                                ),
                                                text: message.to_string(),
                                                sender: Sender::Assistant,
                                                timestamp: chrono::Utc::now(),
                                                pending: false,
                                            });
                                        }
                                        loading.set(false);
                                    }
                                    _ => {}
                                }
                            }
                            ChatWsEvent::Error(message) => {
                                ws_connected.set(false);
                                dioxus_logger::tracing::error!("Chat WS error: {}", message);
                            }
                            ChatWsEvent::Closed => {
                                ws_connected.set(false);
                            }
                        }
                    }

                    TimeoutFuture::new(16).await;
                }
            });
        });
    }

    {
        let ws_event_queue = ws_event_queue.clone();
        use_effect(move || {
            if ws_runtime.read().is_some() {
                return;
            }

            let actor_id = actor_id_signal.to_string();
            let user_id = user_id.to_string();
            let ws_url = build_chat_ws_url(&actor_id, &user_id);

            let ws = match WebSocket::new(&ws_url) {
                Ok(ws) => ws,
                Err(e) => {
                    dioxus_logger::tracing::error!("Chat WS error: {:?}", e);
                    return;
                }
            };
            let closing = Rc::new(Cell::new(false));

            let ws_event_queue_open = ws_event_queue.clone();
            let on_open = Closure::wrap(Box::new(move |_e: Event| {
                ws_event_queue_open
                    .borrow_mut()
                    .push_back(ChatWsEvent::Connected);
            }) as Box<dyn FnMut(Event)>);
            ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

            let ws_event_queue_message = ws_event_queue.clone();
            let on_message = Closure::wrap(Box::new(move |e: MessageEvent| {
                let Ok(text) = e.data().dyn_into::<js_sys::JsString>() else {
                    return;
                };
                let text_str = text.as_string().unwrap_or_default();
                ws_event_queue_message
                    .borrow_mut()
                    .push_back(ChatWsEvent::Message(text_str));
            }) as Box<dyn FnMut(MessageEvent)>);
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

            let ws_event_queue_error = ws_event_queue.clone();
            let on_error = Closure::wrap(Box::new(move |e: ErrorEvent| {
                ws_event_queue_error
                    .borrow_mut()
                    .push_back(ChatWsEvent::Error(e.message()));
            }) as Box<dyn FnMut(ErrorEvent)>);
            ws.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            let ws_event_queue_close = ws_event_queue.clone();
            let closing_for_close = closing.clone();
            let on_close = Closure::wrap(Box::new(move |_e: CloseEvent| {
                if closing_for_close.get() {
                    return;
                }
                ws_event_queue_close
                    .borrow_mut()
                    .push_back(ChatWsEvent::Closed);
            }) as Box<dyn FnMut(CloseEvent)>);
            ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));

            ws_runtime.set(Some(ChatRuntime {
                ws,
                closing,
                _on_open: on_open,
                _on_message: on_message,
                _on_error: on_error,
                _on_close: on_close,
            }));
        });
    }

    // Scroll to bottom when messages change
    use_effect(move || {
        let _ = messages.len();
        // In a real implementation, we'd scroll the messages container to bottom
    });

    let send_message = use_callback(move |_| {
        let text = input_text.to_string();
        if text.trim().is_empty() {
            return;
        }

        let actor_id_val = actor_id_signal.to_string();
        let user_id_val = user_id.to_string();
        let initial_count = messages.read().len();

        // Optimistic update
        let optimistic_msg = ChatMessage {
            id: format!("temp-{}", chrono::Utc::now().timestamp()),
            text: text.clone(),
            sender: Sender::User,
            timestamp: chrono::Utc::now(),
            pending: true,
        };
        messages.push(optimistic_msg);
        {
            let mut entries = thread_entries.read().clone();
            upsert_thread_entry(
                &mut entries,
                ChatThreadEntry {
                    thread_id: actor_id_val.clone(),
                    title: short_thread_title(&actor_id_val),
                    last_preview: text.chars().take(120).collect(),
                    last_updated_ms: chrono::Utc::now().timestamp_millis(),
                },
            );
            persist_chat_thread_entries(&entries);
            thread_entries.set(entries);
        }
        input_text.set(String::new());
        loading.set(true);

        let ws_sent = if let Some(runtime) = ws_runtime.read().as_ref() {
            if runtime.ws.ready_state() == WebSocket::OPEN {
                let msg = serde_json::json!({
                    "type": "message",
                    "text": text.clone(),
                });
                runtime.ws.send_with_str(&msg.to_string()).is_ok()
            } else {
                false
            }
        } else {
            false
        };

        if ws_sent {
            // WebSocket path streams per-chunk updates; avoid duplicate global typing row.
            loading.set(false);
            return;
        }

        spawn(async move {
            match send_chat_message(&actor_id_val, &user_id_val, &text).await {
                Ok(_) => {
                    match fetch_messages(&actor_id_val).await {
                        Ok(msgs) => {
                            let normalized = collapse_tool_messages(msgs);
                            let mut entries = thread_entries.read().clone();
                            upsert_thread_entry(
                                &mut entries,
                                ChatThreadEntry {
                                    thread_id: actor_id_val.clone(),
                                    title: thread_title_from_messages(&actor_id_val, &normalized),
                                    last_preview: latest_message_preview(&normalized),
                                    last_updated_ms: chrono::Utc::now().timestamp_millis(),
                                },
                            );
                            persist_chat_thread_entries(&entries);
                            thread_entries.set(entries);
                            messages.set(normalized);
                        }
                        Err(e) => {
                            dioxus_logger::tracing::error!("Failed to refresh messages: {}", e)
                        }
                    }

                    for _ in 0..6 {
                        TimeoutFuture::new(500).await;
                        if let Ok(msgs) = fetch_messages(&actor_id_val).await {
                            let normalized = collapse_tool_messages(msgs);
                            let has_new_assistant = normalized.len() > initial_count
                                && normalized
                                    .iter()
                                    .any(|m| matches!(m.sender, Sender::Assistant));
                            if has_new_assistant {
                                messages.set(normalized);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to send message: {}", e);
                }
            }
            loading.set(false);
        });
    });

    let onkeydown = use_callback(move |e: KeyboardEvent| {
        if e.key() == Key::Enter && !e.modifiers().shift() {
            e.prevent_default();
            send_message.call(());
        }
    });

    let onclick = use_callback(move |_| {
        send_message.call(());
    });

    let oninput = use_callback(move |e: FormEvent| {
        input_text.set(e.value());
    });

    let on_new_thread = use_callback(move |_| {
        let new_thread_id = format!("thread-{}", chrono::Utc::now().timestamp_millis());
        let mut entries = thread_entries.read().clone();
        upsert_thread_entry(
            &mut entries,
            ChatThreadEntry {
                thread_id: new_thread_id.clone(),
                title: short_thread_title(&new_thread_id),
                last_preview: String::new(),
                last_updated_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
        persist_chat_thread_entries(&entries);
        thread_entries.set(entries);
        actor_id_signal.set(new_thread_id);
        messages.set(Vec::new());
        input_text.set(String::new());
    });

    let on_select_thread = use_callback(move |thread_id: String| {
        let desktop_id_val = desktop_id.clone();
        let window_id_val = window_id.clone();
        spawn(async move {
            let existing_window_id = match fetch_windows(&desktop_id_val).await {
                Ok(windows) => windows
                    .iter()
                    .find(|w| w.app_id == "chat" && w.id == thread_id && w.id != window_id_val)
                    .map(|w| w.id.clone()),
                Err(_) => None,
            };

            if let Some(existing_window_id) = existing_window_id {
                if let Err(e) = focus_window(&desktop_id_val, &existing_window_id).await {
                    dioxus_logger::tracing::error!("Failed to focus existing chat thread: {e}");
                }
                return;
            }

            actor_id_signal.set(thread_id.clone());
            input_text.set(String::new());
            loading.set(false);
        });
    });

    let sorted_threads = {
        let mut entries = thread_entries.read().clone();
        entries.sort_by_key(|entry| -entry.last_updated_ms);
        entries
    };

    rsx! {
        style { {CHAT_STYLES} }

        div {
            class: "chat-container",

            // Header - simplified, no debug info
            div {
                class: "chat-header",
                div {
                    class: "chat-title",
                    span { class: "chat-icon", "💬" }
                    span { "Chat" }
                }
                div {
                    class: "chat-status",
                    span { class: "status-dot", "●" }
                    span { "Online" }
                }
            }

            div {
                class: "chat-body",

                // Thread sidebar
                aside {
                    class: if thread_sidebar_collapsed() {
                        "thread-sidebar collapsed"
                    } else {
                        "thread-sidebar"
                    },
                    button {
                        class: "thread-sidebar-toggle",
                        onclick: move |_| thread_sidebar_collapsed.set(!thread_sidebar_collapsed()),
                        if thread_sidebar_collapsed() { "▶" } else { "◀" }
                    }
                    if !thread_sidebar_collapsed() {
                        div {
                            class: "thread-sidebar-header",
                            span { "Threads" }
                            button {
                                class: "thread-new-button",
                                onclick: on_new_thread,
                                "+ New"
                            }
                        }
                        div {
                            class: "thread-list",
                            for thread in sorted_threads {
                                button {
                                    class: if thread.thread_id == actor_id_signal() {
                                        "thread-item active"
                                    } else {
                                        "thread-item"
                                    },
                                    onclick: {
                                        let thread_id = thread.thread_id.clone();
                                        move |_| on_select_thread.call(thread_id.clone())
                                    },
                                    div { class: "thread-title", "{thread.title}" }
                                    if !thread.last_preview.trim().is_empty() {
                                        div { class: "thread-preview", "{thread.last_preview}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // Messages - scrollable area
                div {
                    class: "messages-scroll-area",
                    div {
                        class: "messages-list",
                        if messages.read().is_empty() {
                            div {
                                class: "empty-state",
                                div { class: "empty-icon", "💬" }
                                p { "Start a conversation" }
                                span { "Type a message below to begin chatting" }
                            }
                        } else {
                            for msg in messages.iter() {
                                MessageBubble { message: msg.clone() }
                            }
                        }
                        if loading() {
                            LoadingIndicator {}
                        }
                    }
                }
            }

            // Input area
            div {
                class: "chat-input-area",
                div {
                    class: "input-wrapper",
                    textarea {
                        class: "chat-textarea",
                        placeholder: "Type a message...",
                        value: "{input_text}",
                        rows: "1",
                        oninput,
                        onkeydown,
                    }
                    button {
                        class: "send-button",
                        disabled: loading() || input_text.read().trim().is_empty(),
                        onclick,
                        if loading() {
                            div {
                                class: "spinner",
                                span { "◐" }
                            }
                        } else {
                            span { "➤" }
                        }
                    }
                }
                div {
                    class: "input-hint",
                    "Press Enter to send, Shift+Enter for new line"
                }
            }
        }
    }
}

#[component]
pub fn MessageBubble(message: ChatMessage) -> Element {
    let is_user = matches!(message.sender, Sender::User);
    let is_system = matches!(message.sender, Sender::System);
    let sender_name = if is_user {
        "You"
    } else if is_system {
        "Tools"
    } else {
        "Assistant"
    };
    let sender_initial = if is_user {
        "Y"
    } else if is_system {
        "T"
    } else {
        "A"
    };

    rsx! {
        div {
            class: if is_user {
                "message-row user-row"
            } else if is_system {
                "message-row system-row"
            } else {
                "message-row assistant-row"
            },

            // Avatar
            div {
                class: if is_user {
                    "avatar user-avatar"
                } else if is_system {
                    "avatar system-avatar"
                } else {
                    "avatar assistant-avatar"
                },
                "{sender_initial}"
            }

            // Message content
            div {
                class: "message-content",

                // Sender name and time
                div {
                    class: "message-header",
                    span { class: "sender-name", "{sender_name}" }
                    span { class: "message-time", "{format_timestamp(message.timestamp)}" }
                    if message.pending {
                        span { class: "pending-badge", "sending..." }
                    }
                }

                // Message bubble
                if let Some(bundle) = parse_assistant_bundle(&message.text) {
                    AssistantMessageWithTools {
                        bundle,
                        pending: message.pending
                    }
                } else if let Some(payload) = parse_tool_payload(&message.text, TOOL_CALL_PREFIX) {
                    ToolCallSection {
                        payload: payload.clone(),
                        force_open: false
                    }
                } else if let Some(payload) = parse_tool_payload(&message.text, TOOL_RESULT_PREFIX) {
                    ToolResultSection {
                        payload: payload.clone(),
                        force_open: false
                    }
                } else if let Some(payload) = parse_tool_payload(&message.text, ACTOR_CALL_PREFIX) {
                    ActorCallSection {
                        payload: payload.clone(),
                        force_open: false
                    }
                } else {
                    div {
                        class: if is_user {
                            "message-bubble user-bubble"
                        } else if is_system {
                            "message-bubble system-bubble"
                        } else {
                            "message-bubble assistant-bubble"
                        },
                        "{message.text}"
                    }
                }
            }
        }
    }
}

#[component]
pub fn LoadingIndicator() -> Element {
    rsx! {
        div {
            class: "message-row assistant-row",
            div {
                class: "avatar assistant-avatar",
                "A"
            }
            div {
                class: "message-content",
                div {
                    class: "message-header",
                    span { class: "sender-name", "Assistant" }
                }
                div {
                    class: "typing-indicator",
                    span {}
                    span {}
                    span {}
                }
            }
        }
    }
}

fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%H:%M").to_string()
}

fn default_thread_sidebar_collapsed() -> bool {
    let width = web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(1200.0);
    width < 1100.0
}

fn load_chat_thread_entries() -> Vec<ChatThreadEntry> {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return Vec::new();
    };
    let Ok(Some(raw)) = storage.get_item("choiros.chat_threads.v1") else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<ChatThreadEntry>>(&raw).unwrap_or_default()
}

fn persist_chat_thread_entries(entries: &[ChatThreadEntry]) {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    if let Ok(payload) = serde_json::to_string(entries) {
        let _ = storage.set_item("choiros.chat_threads.v1", &payload);
    }
}

fn upsert_thread_entry(entries: &mut Vec<ChatThreadEntry>, entry: ChatThreadEntry) {
    if let Some(existing) = entries
        .iter_mut()
        .find(|existing| existing.thread_id == entry.thread_id)
    {
        if !entry.title.trim().is_empty() {
            existing.title = entry.title;
        }
        if !entry.last_preview.trim().is_empty() {
            existing.last_preview = entry.last_preview;
        }
        existing.last_updated_ms = entry.last_updated_ms;
        return;
    }
    entries.push(entry);
}

fn short_thread_title(thread_id: &str) -> String {
    let suffix = thread_id.chars().rev().take(6).collect::<String>();
    format!("Thread {}", suffix.chars().rev().collect::<String>())
}

fn latest_message_preview(messages: &[ChatMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| !matches!(m.sender, Sender::System))
        .map(|m| m.text.chars().take(120).collect::<String>())
        .unwrap_or_default()
}

fn thread_title_from_messages(thread_id: &str, messages: &[ChatMessage]) -> String {
    let seed = messages
        .iter()
        .find(|m| matches!(m.sender, Sender::User))
        .map(|m| m.text.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    if seed.is_empty() {
        return short_thread_title(thread_id);
    }
    seed.chars().take(32).collect()
}

fn parse_chat_response_payload(content: &str) -> ChatResponsePayload {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(content) {
        return ChatResponsePayload {
            text: json
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or(content)
                .to_string(),
            model_used: json
                .get("model_used")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
            model_source: json
                .get("model_source")
                .and_then(|v| v.as_str())
                .map(ToString::to_string),
        };
    }
    ChatResponsePayload {
        text: content.to_string(),
        model_used: None,
        model_source: None,
    }
}

fn parse_tool_payload(text: &str, prefix: &str) -> Option<serde_json::Value> {
    let payload = text.strip_prefix(prefix)?;
    serde_json::from_str::<serde_json::Value>(payload).ok()
}

fn parse_assistant_bundle(text: &str) -> Option<AssistantBundle> {
    let payload = text.strip_prefix(ASSISTANT_BUNDLE_PREFIX)?;
    serde_json::from_str::<AssistantBundle>(payload).ok()
}

fn encode_assistant_bundle_text(response_text: &str, tools: Vec<ToolEntry>) -> String {
    if tools.is_empty() {
        return response_text.to_string();
    }
    let bundle = AssistantBundle {
        text: response_text.to_string(),
        thinking: Vec::new(),
        tools,
        model_used: None,
        model_source: None,
    };
    match serde_json::to_string(&bundle) {
        Ok(payload) => format!("{ASSISTANT_BUNDLE_PREFIX}{payload}"),
        Err(_) => response_text.to_string(),
    }
}

fn collapse_tool_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut out = Vec::with_capacity(messages.len());
    let mut pending_tools: VecDeque<ToolEntry> = VecDeque::new();

    for msg in messages {
        if matches!(msg.sender, Sender::System) {
            if let Some(payload) = parse_tool_payload(&msg.text, TOOL_CALL_PREFIX) {
                pending_tools.push_back(ToolEntry {
                    kind: "call".to_string(),
                    payload,
                });
                continue;
            }
            if let Some(payload) = parse_tool_payload(&msg.text, TOOL_RESULT_PREFIX) {
                pending_tools.push_back(ToolEntry {
                    kind: "result".to_string(),
                    payload,
                });
                continue;
            }
            if let Some(payload) = parse_tool_payload(&msg.text, ACTOR_CALL_PREFIX) {
                pending_tools.push_back(ToolEntry {
                    kind: "actor_call".to_string(),
                    payload,
                });
                continue;
            }
        }

        if matches!(msg.sender, Sender::Assistant) && !pending_tools.is_empty() {
            let tools = pending_tools.drain(..).collect::<Vec<_>>();
            let bundled_text = encode_assistant_bundle_text(&msg.text, tools);
            out.push(ChatMessage {
                text: bundled_text,
                ..msg
            });
            continue;
        }

        while let Some(tool) = pending_tools.pop_front() {
            let prefix = match tool.kind.as_str() {
                "call" => TOOL_CALL_PREFIX,
                "result" => TOOL_RESULT_PREFIX,
                "actor_call" => ACTOR_CALL_PREFIX,
                _ => TOOL_RESULT_PREFIX,
            };
            out.push(ChatMessage {
                id: format!("legacy-tool-{}", chrono::Utc::now().timestamp_millis()),
                text: format!("{prefix}{}", tool.payload),
                sender: Sender::System,
                timestamp: chrono::Utc::now(),
                pending: false,
            });
        }
        out.push(msg);
    }

    out
}

fn format_tool_args(raw: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| raw.to_string()),
        Err(_) => raw.to_string(),
    }
}

#[component]
fn AssistantMessageWithTools(bundle: AssistantBundle, pending: bool) -> Element {
    let latest_thinking = bundle.thinking.last().cloned().unwrap_or_default();
    let mut tool_activity_open = use_signal(|| false);
    let mut expand_all = use_signal(|| false);
    rsx! {
        if let Some(model_used) = bundle.model_used.clone() {
            p {
                class: "tool-meta",
                if let Some(model_source) = bundle.model_source.clone() {
                    "Model: {model_used} ({model_source})"
                } else {
                    "Model: {model_used}"
                }
            }
        }
        if !latest_thinking.is_empty() && pending {
            p {
                class: "tool-meta",
                "Thinking: {latest_thinking}"
            }
        }
        if !bundle.tools.is_empty() {
            div {
                class: "tool-details",
                div {
                    class: "tool-activity-header",
                    button {
                        class: "tool-activity-toggle",
                        onclick: move |_| tool_activity_open.set(!tool_activity_open()),
                        if tool_activity_open() {
                            "▼ Tool activity ({bundle.tools.len()})"
                        } else {
                            "▶ Tool activity ({bundle.tools.len()})"
                        }
                    }
                    button {
                        class: "tool-action-button",
                        onclick: move |_| {
                            let next = !expand_all();
                            expand_all.set(next);
                            if next {
                                // "Expand all" should be a single-click reveal of everything.
                                tool_activity_open.set(true);
                            }
                        },
                        if expand_all() {
                            "Collapse all"
                        } else {
                            "Expand all"
                        }
                    }
                }
                if tool_activity_open() {
                    div {
                        class: "tool-body",
                        for tool in bundle.tools {
                            if tool.kind == "call" {
                                ToolCallSection {
                                    payload: tool.payload.clone(),
                                    force_open: expand_all()
                                }
                            } else if tool.kind == "actor_call" {
                                ActorCallSection {
                                    payload: tool.payload.clone(),
                                    force_open: expand_all()
                                }
                            } else {
                                ToolResultSection {
                                    payload: tool.payload.clone(),
                                    force_open: expand_all()
                                }
                            }
                        }
                    }
                }
            }
        }
        if !bundle.text.is_empty() {
            div {
                class: "message-bubble assistant-bubble",
                "{bundle.text}"
            }
        } else if pending {
            div {
                class: "typing-indicator",
                span {}
                span {}
                span {}
            }
        }
    }
}

fn has_pending_assistant_bundle(messages: &[ChatMessage]) -> bool {
    messages.last().is_some_and(|msg| {
        msg.pending
            && matches!(msg.sender, Sender::Assistant)
            && parse_assistant_bundle(&msg.text).is_some()
    })
}

fn update_or_create_pending_assistant_bundle<F>(messages: &mut Vec<ChatMessage>, f: F)
where
    F: FnOnce(&mut AssistantBundle),
{
    if has_pending_assistant_bundle(messages) {
        if let Some(last) = messages.last_mut() {
            let mut bundle =
                parse_assistant_bundle(&last.text).unwrap_or_else(empty_assistant_bundle);
            f(&mut bundle);
            last.text = encode_assistant_bundle(bundle);
        }
        return;
    }

    let mut bundle = empty_assistant_bundle();
    f(&mut bundle);
    messages.push(ChatMessage {
        id: format!("assistant-stream-{}", chrono::Utc::now().timestamp_millis()),
        text: encode_assistant_bundle(bundle),
        sender: Sender::Assistant,
        timestamp: chrono::Utc::now(),
        pending: true,
    });
}

fn mark_last_pending_assistant_complete(messages: &mut [ChatMessage]) {
    if let Some(last) = messages.last_mut() {
        if last.pending && matches!(last.sender, Sender::Assistant) {
            last.pending = false;
        }
    }
}

fn empty_assistant_bundle() -> AssistantBundle {
    AssistantBundle {
        text: String::new(),
        thinking: Vec::new(),
        tools: Vec::new(),
        model_used: None,
        model_source: None,
    }
}

fn encode_assistant_bundle(bundle: AssistantBundle) -> String {
    match serde_json::to_string(&bundle) {
        Ok(payload) => format!("{ASSISTANT_BUNDLE_PREFIX}{payload}"),
        Err(_) => String::new(),
    }
}

#[component]
fn ToolCallSection(payload: serde_json::Value, force_open: bool) -> Element {
    let tool_name = payload
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown_tool");
    let tool_args = payload
        .get("tool_args")
        .and_then(|v| v.as_str())
        .unwrap_or("{}");
    let reasoning = payload
        .get("reasoning")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let tool_args_formatted = format_tool_args(tool_args);

    rsx! {
        details {
            class: "tool-details",
            open: force_open,
            summary {
                class: "tool-summary",
                "Tool call: {tool_name}"
            }
            div {
                class: "tool-body",
                if !reasoning.is_empty() {
                    p { class: "tool-meta", "Reasoning: {reasoning}" }
                }
                p { class: "tool-label", "Arguments" }
                pre { class: "tool-pre", "{tool_args_formatted}" }
            }
        }
    }
}

#[component]
fn ToolResultSection(payload: serde_json::Value, force_open: bool) -> Element {
    let tool_name = payload
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown_tool");
    let success = payload
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let output = payload
        .get("output")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let error = payload
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let status = if success { "success" } else { "failed" };
    let details_text = if !error.is_empty() {
        error.to_string()
    } else {
        output.to_string()
    };

    rsx! {
        details {
            class: "tool-details",
            open: force_open,
            summary {
                class: "tool-summary",
                "Tool result: {tool_name} ({status})"
            }
            div {
                class: "tool-body",
                if !details_text.trim().is_empty() {
                    pre { class: "tool-pre", "{details_text}" }
                } else {
                    p { class: "tool-meta", "No output" }
                }
            }
        }
    }
}

#[component]
fn ActorCallSection(payload: serde_json::Value, force_open: bool) -> Element {
    let phase = payload
        .get("phase")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("event_type").and_then(|v| v.as_str()))
        .or_else(|| payload.get("status").and_then(|v| v.as_str()))
        .unwrap_or("worker_update");
    let message = payload
        .get("message")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("status").and_then(|v| v.as_str()))
        .unwrap_or("Worker update");
    let reasoning = payload
        .get("reasoning")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let command = payload
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let output_excerpt = payload
        .get("output_excerpt")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let exit_code = payload
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .map(|v| v.to_string())
        .unwrap_or_default();

    rsx! {
        details {
            class: "tool-details",
            open: force_open,
            summary {
                class: "tool-summary",
                "Actor update: {phase}"
            }
            div {
                class: "tool-body",
                p { class: "tool-meta", "{message}" }
                if !reasoning.is_empty() {
                    p { class: "tool-meta", "Reasoning: {reasoning}" }
                }
                if !command.is_empty() {
                    p { class: "tool-label", "Command" }
                    pre { class: "tool-pre", "{command}" }
                }
                if !output_excerpt.is_empty() {
                    p { class: "tool-label", "Output excerpt" }
                    pre { class: "tool-pre", "{output_excerpt}" }
                }
                if !exit_code.is_empty() {
                    p { class: "tool-meta", "Exit code: {exit_code}" }
                }
            }
        }
    }
}

fn clear_pending_user_message(messages: &mut Vec<ChatMessage>) {
    if let Some(msg) = messages
        .iter_mut()
        .rev()
        .find(|m| matches!(m.sender, Sender::User) && m.pending)
    {
        msg.pending = false;
    }
}

fn build_chat_ws_url(actor_id: &str, user_id: &str) -> String {
    let ws_base = http_to_ws_url(crate::api::api_base());
    format!("{}/ws/chat/{}/{}", ws_base, actor_id, user_id)
}

impl Drop for ChatRuntime {
    fn drop(&mut self) {
        self.closing.set(true);
        self.ws.set_onopen(None);
        self.ws.set_onmessage(None);
        self.ws.set_onerror(None);
        self.ws.set_onclose(None);
        let _ = self.ws.close();
    }
}

fn http_to_ws_url(http_url: &str) -> String {
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
