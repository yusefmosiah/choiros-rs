use dioxus::prelude::*;
use chrono::{DateTime, Utc};
use shared_types::{ChatMessage, Sender};

use crate::api::{fetch_messages, send_chat_message};

#[component]
pub fn ChatView(actor_id: String) -> Element {
    let mut messages = use_signal(Vec::<ChatMessage>::new);
    let mut input_text = use_signal(String::new);
    let user_id = use_signal(|| "user-1".to_string());
    let mut loading = use_signal(|| false);
    let actor_id_signal = use_signal(|| actor_id.clone());
    
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
                        Err(e) => dioxus_logger::tracing::error!("Failed to refresh messages: {}", e),
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
        if e.key() == Key::Enter {
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
        div {
            class: "chat-container",
            
            // Header
            div {
                class: "chat-header",
                h1 { "ChoirOS Chat" }
                p { class: "actor-id", "Actor: {actor_id_signal}" }
            }
            
            // Messages
            div {
                class: "messages-container",
                for msg in messages.iter() {
                    MessageBubble { message: msg.clone() }
                }
            }
            
            // Input
            div {
                class: "input-container",
                input {
                    class: "message-input",
                    placeholder: "Type a message...",
                    value: "{input_text}",
                    oninput,
                    onkeydown,
                }
                button {
                    class: "send-button",
                    disabled: loading(),
                    onclick,
                    if loading() {
                        "Sending..."
                    } else {
                        "Send"
                    }
                }
            }
        }
    }
}

#[component]
pub fn MessageBubble(message: ChatMessage) -> Element {
    let is_user = matches!(message.sender, Sender::User);
    let pending_class = if message.pending { " pending" } else { "" };
    
    rsx! {
        div {
            class: if is_user { "message-wrapper user" } else { "message-wrapper assistant" },
            div {
                class: if is_user { format!("message-bubble user-bubble{}", pending_class) } else { format!("message-bubble assistant-bubble{}", pending_class) },
                
                p { "{message.text}" }
                
                div {
                    class: "message-meta",
                    span { "{format_timestamp(message.timestamp)}" }
                    if message.pending {
                        span { " • Sending..." }
                    }
                }
            }
        }
    }
}

fn format_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.format("%H:%M").to_string()
}
