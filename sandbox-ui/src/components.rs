use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use shared_types::{ChatMessage, Sender};

use crate::api::{fetch_messages, send_chat_message};

#[component]
pub fn ChatView(actor_id: String) -> Element {
    let mut messages = use_signal(Vec::<ChatMessage>::new);
    let mut input_text = use_signal(String::new);
    let user_id = use_signal(|| "user-1".to_string());
    let mut loading = use_signal(|| false);
    let actor_id_signal = use_signal(|| actor_id.clone());
    let _messages_end_ref = use_signal(|| None::<dioxus::prelude::Element>);

    // Load messages on mount
    use_effect(move || {
        let actor_id = actor_id_signal.to_string();
        spawn(async move {
            match fetch_messages(&actor_id).await {
                Ok(msgs) => {
                    messages.set(msgs);
                }
                Err(e) => {
                    dioxus_logger::tracing::error!("Failed to fetch messages: {}", e);
                }
            }
        });
    });

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

        // Optimistic update
        let optimistic_msg = ChatMessage {
            id: format!("temp-{}", chrono::Utc::now().timestamp()),
            text: text.clone(),
            sender: Sender::User,
            timestamp: chrono::Utc::now(),
            pending: true,
        };
        messages.push(optimistic_msg);
        input_text.set(String::new());
        loading.set(true);

        spawn(async move {
            match send_chat_message(&actor_id_val, &user_id_val, &text).await {
                Ok(_) => {
                    // Refresh messages
                    match fetch_messages(&actor_id_val).await {
                        Ok(msgs) => messages.set(msgs),
                        Err(e) => {
                            dioxus_logger::tracing::error!("Failed to refresh messages: {}", e)
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
    let sender_name = if is_user { "You" } else { "Assistant" };
    let sender_initial = if is_user { "Y" } else { "A" };

    rsx! {
        div {
            class: if is_user { "message-row user-row" } else { "message-row assistant-row" },

            // Avatar
            div {
                class: if is_user { "avatar user-avatar" } else { "avatar assistant-avatar" },
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
                div {
                    class: if is_user { "message-bubble user-bubble" } else { "message-bubble assistant-bubble" },
                    "{message.text}"
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

// Chat-specific CSS styles
const CHAT_STYLES: &str = r#"
/* Chat Container */
.chat-container {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--chat-bg, #0f172a);
    overflow: hidden;
}

/* Header */
.chat-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    background: var(--chat-header-bg, #1e293b);
    border-bottom: 1px solid var(--border-color, #334155);
    flex-shrink: 0;
}

.chat-title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 600;
    color: var(--text-primary, #f8fafc);
}

.chat-icon {
    font-size: 1.25rem;
}

.chat-status {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    font-size: 0.75rem;
    color: var(--text-secondary, #94a3b8);
}

.status-dot {
    color: var(--success-bg, #10b981);
    font-size: 0.5rem;
}

/* Messages Scroll Area */
.messages-scroll-area {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    padding: 1rem;
    scroll-behavior: smooth;
}

.messages-scroll-area::-webkit-scrollbar {
    width: 6px;
}

.messages-scroll-area::-webkit-scrollbar-track {
    background: transparent;
}

.messages-scroll-area::-webkit-scrollbar-thumb {
    background: var(--border-color, #334155);
    border-radius: 3px;
}

.messages-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 100%;
}

/* Empty State */
.empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 3rem 1rem;
    color: var(--text-muted, #64748b);
    text-align: center;
}

.empty-icon {
    font-size: 3rem;
    margin-bottom: 1rem;
    opacity: 0.5;
}

.empty-state p {
    font-weight: 500;
    color: var(--text-secondary, #94a3b8);
    margin: 0 0 0.25rem 0;
}

.empty-state span {
    font-size: 0.875rem;
}

/* Message Row */
.message-row {
    display: flex;
    gap: 0.75rem;
    max-width: 100%;
}

.user-row {
    flex-direction: row-reverse;
}

.assistant-row {
    flex-direction: row;
}

/* Avatar */
.avatar {
    width: 2rem;
    height: 2rem;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
    font-weight: 600;
    flex-shrink: 0;
}

.user-avatar {
    background: var(--accent-bg, #3b82f6);
    color: white;
}

.assistant-avatar {
    background: var(--bg-secondary, #1e293b);
    color: var(--text-secondary, #94a3b8);
    border: 1px solid var(--border-color, #334155);
}

/* Message Content */
.message-content {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    max-width: calc(100% - 3rem);
}

.user-row .message-content {
    align-items: flex-end;
}

.assistant-row .message-content {
    align-items: flex-start;
}

/* Message Header */
.message-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.75rem;
}

.user-row .message-header {
    flex-direction: row-reverse;
}

.sender-name {
    font-weight: 500;
    color: var(--text-secondary, #94a3b8);
}

.message-time {
    color: var(--text-muted, #64748b);
}

.pending-badge {
    color: var(--warning-bg, #f59e0b);
    font-style: italic;
}

/* Message Bubble */
.message-bubble {
    padding: 0.75rem 1rem;
    border-radius: 1rem;
    font-size: 0.9375rem;
    line-height: 1.5;
    word-wrap: break-word;
    max-width: 100%;
}

.user-bubble {
    background: var(--accent-bg, #3b82f6);
    color: white;
    border-bottom-right-radius: 0.25rem;
}

.assistant-bubble {
    background: var(--bg-secondary, #1e293b);
    color: var(--text-primary, #f8fafc);
    border: 1px solid var(--border-color, #334155);
    border-bottom-left-radius: 0.25rem;
}

/* Typing Indicator */
.typing-indicator {
    display: flex;
    gap: 0.25rem;
    padding: 1rem;
    background: var(--bg-secondary, #1e293b);
    border: 1px solid var(--border-color, #334155);
    border-radius: 1rem;
    border-bottom-left-radius: 0.25rem;
    width: fit-content;
}

.typing-indicator span {
    width: 0.5rem;
    height: 0.5rem;
    background: var(--text-muted, #64748b);
    border-radius: 50%;
    animation: typing-bounce 1.4s infinite ease-in-out both;
}

.typing-indicator span:nth-child(1) { animation-delay: -0.32s; }
.typing-indicator span:nth-child(2) { animation-delay: -0.16s; }

@keyframes typing-bounce {
    0%, 80%, 100% { transform: scale(0); }
    40% { transform: scale(1); }
}

/* Chat Input Area */
.chat-input-area {
    padding: 0.75rem 1rem;
    background: var(--chat-header-bg, #1e293b);
    border-top: 1px solid var(--border-color, #334155);
    flex-shrink: 0;
}

.input-wrapper {
    display: flex;
    gap: 0.5rem;
    align-items: flex-end;
}

.chat-textarea {
    flex: 1;
    padding: 0.75rem 1rem;
    background: var(--input-bg, #0f172a);
    color: var(--text-primary, #f8fafc);
    border: 1px solid var(--border-color, #334155);
    border-radius: 1.5rem;
    font-size: 0.9375rem;
    font-family: inherit;
    resize: none;
    outline: none;
    min-height: 2.75rem;
    max-height: 8rem;
    line-height: 1.25;
    transition: border-color 0.2s, box-shadow 0.2s;
}

.chat-textarea:focus {
    border-color: var(--accent-bg, #3b82f6);
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.2);
}

.chat-textarea::placeholder {
    color: var(--text-muted, #64748b);
}

.send-button {
    width: 2.75rem;
    height: 2.75rem;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-bg, #3b82f6);
    color: white;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    font-size: 1.25rem;
    transition: all 0.2s;
    flex-shrink: 0;
}

.send-button:hover:not(:disabled) {
    background: var(--accent-bg-hover, #2563eb);
    transform: scale(1.05);
}

.send-button:disabled {
    background: var(--border-color, #334155);
    color: var(--text-muted, #64748b);
    cursor: not-allowed;
}

.send-button .spinner {
    animation: spin 1s linear infinite;
}

@keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
}

.input-hint {
    margin-top: 0.5rem;
    font-size: 0.75rem;
    color: var(--text-muted, #64748b);
    text-align: center;
}
"#;
