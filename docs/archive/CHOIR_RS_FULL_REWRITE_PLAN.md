# ChoirOS Full Rust Rewrite: 6-Hour Implementation Plan

**Status:** Ready to execute  
**Paradigm:** "Automatic Computer" - Self-modifying, multi-tenant, event-sourced  
**Stack:** Rust everywhere (Yew WASM frontend, Actix actors/backend, SQLite events)  

---

## Executive Summary

**Why 6 hours, not 6 weeks:**

1. **Single workspace** - Shared types between frontend/backend (no serialization mismatches)
2. **Copy-paste patterns** - Yew has React-like hooks, Actix has familiar actor patterns
3. **Event sourcing simplifies** - One SQLite table, projections rebuild automatically
4. **No complex migrations** - Greenfield repo, no legacy compatibility needed
5. **MVP scope** - Desktop shell + 2 apps (chat, writer), add more later

**Architecture - Single Binary Per Sandbox:**

Yew compiles to WebAssembly that runs in the browser. Actix is the server. They communicate via HTTP/WebSocket.

```
Browser                              EC2 Instance
┌──────────────┐                    ┌──────────────────────┐
│ Yew WASM     │  ←──HTTP/WS──→    │ Hypervisor (port     │
│ (UI runs in  │                    │ 8001)                │
│ browser)     │                    │ • Auth (WebAuthn)    │
└──────────────┘                    │ • Routes to sandbox  │
                                    └──────────┬───────────┘
                                               │
                                    ┌──────────┴──────────┐
                                    │ User Sandbox        │
                                    │ (Docker container)  │
                                    │                     │
                                    │ ┌─────────────────┐ │
                                    │ │ Actix Web       │ │
                                    │ │ (single binary) │ │
                                    │ │                 │ │
                                    │ │ • /api/* → API  │ │
                                    │ │ • /ws → WebSock │ │
                                    │ │ • /* → static   │ │
                                    │ │   (Yew files)   │ │
                                    │ └─────────────────┘ │
                                    │                     │
                                    │ ┌─────────────────┐ │
                                    │ │ Chat Actor      │ │
                                    │ │ Event Store     │ │
                                    │ │ SQLite          │ │
                                    │ └─────────────────┘ │
                                    └─────────────────────┘
```

**Key:** One Actix binary serves BOTH API and static Yew files. No separate frontend container.

**Strategy: Chat-First MVP**

Build chat first, then use chat to control other apps via tool calls:

```
Phase 1: Chat MVP
├─ Chat actor + event store
├─ Basic tool calling (bash, file ops)
└─ Yew chat UI

Phase 2: Chat as orchestrator
├─ Add WriterActor
├─ Tool: chat calls writer_agent.create_doc("Write about X")
└─ Chat becomes the primary interface

Phase 3: Agent mesh
├─ Any agent can call any other agent
└─ Full "automatic computer" paradigm
```

---

## Phase 0: Project Bootstrap (30 minutes)

### 0.1 Create Workspace

```bash
mkdir choiros-rs
cd choiros-rs
cargo init --name choir-workspace

# Edit Cargo.toml to workspace
```toml
[workspace]
members = ["shared-types", "hypervisor", "sandbox", "sandbox-ui"]
resolver = "2"

[workspace.dependencies]
actix = "0.13"
actix-web = "4.9"
actix-rt = "2.10"
tokio = { version = "1.40", features = ["full"] }
yew = { version = "0.21", features = ["csr"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate"] }
uuid = { version = "1.11", features = ["v4", "serde"] }
tracing = "0.1"
```

### 0.2 Create Justfile

```makefile
# Justfile - Task runner
default:
    @just --list

# Build everything
build:
    cargo build --release

# Run hypervisor
dev-hypervisor:
    cargo watch -p hypervisor -x 'run -p hypervisor'

# Run sandbox with UI
dev-sandbox:
    cd sandbox-ui && trunk serve --port 5173 &
    cargo watch -p sandbox -x 'run -p sandbox'

# Build UI for production (embeds in sandbox binary)
build-ui:
    cd sandbox-ui && trunk build --release
    cp sandbox-ui/dist sandbox/static/

# Test everything
test:
    cargo test --workspace

# Check code quality
check:
    cargo fmt --check
    cargo clippy --workspace -- -D warnings
```

---

## Phase 1: Shared Types (30 minutes)

This is the magic - types shared between frontend WASM and backend native:

```rust
// shared-types/src/lib.rs
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

// Events - shared between all components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub seq: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
    pub actor_id: String,
}

// Actor messages - sent between actors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatMsg {
    UserTyped { text: String, window_id: String },
    AssistantReply { text: String },
    ToolCall { tool: String, args: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WriterMsg {
    CreateDoc { title: String },
    EditFile { path: String, content: String },
    FileChanged { path: String },  // From event subscription
}

// UI state - sent from backend to frontend
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DesktopState {
    pub windows: Vec<WindowState>,
    pub active_window: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub id: String,
    pub app_type: String, // "chat", "writer", "mail"
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub minimized: bool,
}

// API types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}
```

**Why this saves time:**
- Frontend and backend use SAME structs
- No protobuf, no GraphQL schemas, no OpenAPI
- Compiler checks both sides stay in sync
- JSON serialization just works

---

## Phase 2: Hypervisor - Edge Router (1 hour)

Ultra-thin - just auth and WebSocket routing:

```rust
// hypervisor/src/main.rs
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Error};
use actix_web::middleware::Logger;
use shared_types::*;

mod auth;
mod routing;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    
    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default())
            .route("/health", web::get().to(health))
            .route("/ws", web::get().to(websocket_handler))
    })
    .bind("0.0.0.0:8001")?
    .run()
    .await
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "hypervisor"
    }))
}

// WebSocket handler - just routes to sandbox
async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    // 1. Authenticate (WebAuthn/passkey)
    let user_id = auth::authenticate(&req).await?;
    
    // 2. Find or spawn user's sandbox
    let sandbox_addr = routing::get_or_create_sandbox(&user_id).await;
    
    // 3. Proxy WebSocket to sandbox
    // This is a transparent tunnel - hypervisor doesn't parse messages
    actix_web_actors::ws::start(
        ProxyActor { sandbox_addr },
        &req,
        stream,
    )
}

// Proxy actor - passes through all messages
use actix::{Actor, StreamHandler};
use actix_web_actors::ws;

struct ProxyActor {
    sandbox_addr: String, // e.g., "127.0.0.1:9001"
}

impl Actor for ProxyActor {
    type Context = ws::WebsocketContext<Self>;
}

// Messages from browser → forward to sandbox
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for ProxyActor {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Text(text)) => {
                // Forward to sandbox via internal HTTP
                // (In production: direct TCP, for now: HTTP post)
            }
            Ok(ws::Message::Binary(bin)) => {
                // Forward binary
            }
            Ok(ws::Message::Close(_)) => {
                ctx.stop();
            }
            _ => (),
        }
    }
}
```

**Why this is 200 lines:**
- No business logic
- No state (except routing table)
- Just auth + spawn sandbox + proxy bytes

---

## Phase 3: Sandbox - The "Computer" (2 hours)

This is the full ChoirOS stack in one binary:

```rust
// sandbox/src/main.rs
use actix_web::{web, App, HttpServer, HttpResponse, middleware};
use actix::Actor;

mod actors;
mod event_store;
mod baml;
mod api;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // 1. Init SQLite event store
    let event_store = event_store::EventStoreActor::new("./data/events.db")
        .await
        .expect("Failed to init event store")
        .start();
    
    // 2. Start actor system
    let chat_actor = actors::ChatActor::new(event_store.clone()).start();
    let writer_actor = actors::WriterActor::new(event_store.clone()).start();
    let desktop_actor = actors::DesktopActor::new(event_store.clone()).start();
    
    // 3. Start web server (serves both API and static UI)
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(event_store.clone()))
            .app_data(web::Data::new(chat_actor.clone()))
            .app_data(web::Data::new(writer_actor.clone()))
            .app_data(web::Data::new(desktop_actor.clone()))
            // API routes
            .service(api::events_stream)
            .service(api::send_message)
            .service(api::get_desktop_state)
            // Static files (Yew UI)
            .service(actix_files::Files::new("/", "./static").index_file("index.html"))
    })
    .bind("0.0.0.0:8080")?  // Hypervisor proxies to this
    .run()
    .await
}
```

### 3.1 Event Store Actor

```rust
// sandbox/src/event_store.rs
use actix::{Actor, Handler, Context};
use sqlx::SqlitePool;
use shared_types::Event;

pub struct EventStoreActor {
    pool: SqlitePool,
    subscribers: Vec<actix::Recipient<EventPublished>>,
}

impl Actor for EventStoreActor {
    type Context = Context<Self>;
}

#[derive(actix::Message)]
#[rtype(result = "Result<Event, sqlx::Error>")]
pub struct AppendEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub actor_id: String,
}

impl Handler<AppendEvent> for EventStoreActor {
    type Result = Result<Event, sqlx::Error>;
    
    fn handle(&mut self, msg: AppendEvent, _ctx: &mut Context<Self>) -> Self::Result {
        // SQLite insert
        let event = sqlx::query_as::<_, Event>(
            "INSERT INTO events (event_type, payload, actor_id, timestamp) 
             VALUES (?1, ?2, ?3, datetime('now'))
             RETURNING *"
        )
        .bind(&msg.event_type)
        .bind(&msg.payload)
        .bind(&msg.actor_id)
        .fetch_one(&self.pool)
        .await?;
        
        // Notify subscribers (for live UI updates)
        for sub in &self.subscribers {
            sub.do_send(EventPublished(event.clone()));
        }
        
        Ok(event)
    }
}

// Subscribe to events (for projections)
#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct Subscribe {
    pub recipient: actix::Recipient<EventPublished>,
}

impl Handler<Subscribe> for EventStoreActor {
    type Result = ();
    fn handle(&mut self, msg: Subscribe, _ctx: &mut Context<Self>) {
        self.subscribers.push(msg.recipient);
    }
}

#[derive(actix::Message, Clone)]
#[rtype(result = "()")]
pub struct EventPublished(pub Event);
```

### 3.2 Chat Actor

```rust
// sandbox/src/actors/chat.rs
use actix::{Actor, Handler, Context};
use shared_types::{ChatMsg, Event};

pub struct ChatActor {
    event_store: actix::Addr<super::EventStoreActor>,
    messages: Vec<ChatMessage>,
}

impl Actor for ChatActor {
    type Context = Context<Self>;
    
    fn started(&mut self, ctx: &mut Self::Context) {
        // Subscribe to events we care about
        let self_recipient = ctx.address().recipient();
        self.event_store.do_send(super::Subscribe {
            recipient: self_recipient,
        });
    }
}

impl Handler<ChatMsg> for ChatActor {
    type Result = ();
    
    fn handle(&mut self, msg: ChatMsg, _ctx: &mut Context<Self>) {
        match msg {
            ChatMsg::UserTyped { text, window_id } => {
                // Store event
                self.event_store.do_send(super::AppendEvent {
                    event_type: "chat.user_msg".to_string(),
                    payload: serde_json::json!({
                        "text": text,
                        "window_id": window_id
                    }),
                    actor_id: "chat".to_string(),
                });
                
                // Trigger LLM response
                if text.starts_with("/") {
                    // Command
                } else {
                    // Normal message - maybe respond?
                }
            }
            _ => {}
        }
    }
}

// Handle events from event store (projections)
impl Handler<super::EventPublished> for ChatActor {
    type Result = ();
    
    fn handle(&mut self, msg: super::EventPublished, _ctx: &mut Context<Self>) {
        // Update local state based on event
        match msg.0.event_type.as_str() {
            "chat.user_msg" => {
                // Add to messages vec
            }
            "chat.assistant_msg" => {
                // Add AI response
            }
            _ => {}
        }
    }
}
```

---

## Phase 4: Yew Desktop UI (2 hours)

### 4.1 Window Manager

```rust
// sandbox-ui/src/desktop.rs
use yew::prelude::*;
use shared_types::*;

#[function_component(Desktop)]
pub fn desktop() -> Html {
    let windows = use_state(Vec::new);
    let active_window = use_state(|| None::<String>);
    
    // Connect to backend via WebSocket
    let ws = use_web_socket("ws://localhost:8001/ws");
    
    // Listen for desktop state updates
    use_effect_with(ws.message, move |msg| {
        if let Some(Ok(text)) = msg {
            if let Ok(state) = serde_json::from_str::<DesktopState>(&text) {
                windows.set(state.windows);
                active_window.set(state.active_window);
            }
        }
        || ()
    });
    
    html! {
        <div class="desktop" style="width: 100vw; height: 100vh; background: linear-gradient(135deg, #1e3c72 0%, #2a5298 100%);">
            // Taskbar
            <div class="taskbar" style="position: fixed; bottom: 0; width: 100%; height: 48px; background: rgba(0,0,0,0.8); display: flex; align-items: center; padding: 0 16px;">
                <button onclick={open_chat}> {"Chat"} </button>
                <button onclick={open_writer}> {"Writer"} </button>
            </div>
            
            // Windows
            {windows.iter().map(|window| {
                html! {
                    <Window 
                        key={window.id.clone()}
                        state={window.clone()}
                        is_active={active_window.as_ref() == Some(&window.id)}
                        on_activate={activate_window.clone()}
                        on_close={close_window.clone()}
                        on_move={move_window.clone()}
                    />
                }
            }).collect::<Html>()}
        </div>
    }
}
```

### 4.2 Draggable Window Component

```rust
// sandbox-ui/src/window.rs
use yew::prelude::*;
use shared_types::WindowState;

#[derive(Properties, PartialEq)]
pub struct WindowProps {
    pub state: WindowState,
    pub is_active: bool,
    pub on_activate: Callback<String>,
    pub on_close: Callback<String>,
    pub on_move: Callback<(String, i32, i32)>,
}

#[function_component(Window)]
pub fn window(props: &WindowProps) -> Html {
    let dragging = use_state(|| false);
    let drag_offset = use_state(|| (0, 0));
    let position = use_state(|| (props.state.x, props.state.y));
    
    let on_mouse_down = {
        let dragging = dragging.clone();
        let drag_offset = drag_offset.clone();
        let position = position.clone();
        let on_activate = props.on_activate.clone();
        let window_id = props.state.id.clone();
        
        Callback::from(move |e: MouseEvent| {
            dragging.set(true);
            drag_offset.set((e.client_x() - position.0, e.client_y() - position.1));
            on_activate.emit(window_id.clone());
        })
    };
    
    let on_mouse_move = {
        let dragging = dragging.clone();
        let drag_offset = drag_offset.clone();
        let position = position.clone();
        let on_move = props.on_move.clone();
        let window_id = props.state.id.clone();
        
        Callback::from(move |e: MouseEvent| {
            if *dragging {
                let new_x = e.client_x() - drag_offset.0;
                let new_y = e.client_y() - drag_offset.1;
                position.set((new_x, new_y));
                on_move.emit((window_id.clone(), new_x, new_y));
            }
        })
    };
    
    let style = format!(
        "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; 
         background: white; border-radius: 8px; box-shadow: 0 4px 20px rgba(0,0,0,0.3);
         display: flex; flex-direction: column; z-index: {};",
        position.0, position.1, props.state.width, props.state.height,
        if props.is_active { 100 } else { 1 }
    );
    
    html! {
        <div {style} onmousedown={on_mouse_down} onmousemove={on_mouse_move}>
            // Title bar
            <div style="background: #f0f0f0; padding: 8px; border-radius: 8px 8px 0 0; cursor: move; display: flex; justify-content: space-between;">
                <span>{&props.state.title}</span>
                <button onclick={close_window}>{"×"}</button>
            </div>
            
            // Content - dispatch to app component
            <div style="flex: 1; overflow: auto;">
                {match props.state.app_type.as_str() {
                    "chat" => html! { <ChatApp window_id={props.state.id.clone()} /> },
                    "writer" => html! { <WriterApp window_id={props.state.id.clone()} /> },
                    _ => html! { <div>{"Unknown app"}</div> },
                }}
            </div>
        </div>
    }
}
```

### 4.3 Chat App

```rust
// sandbox-ui/src/apps/chat.rs
use yew::prelude::*;
use shared_types::*;

#[derive(Properties, PartialEq)]
pub struct ChatAppProps {
    pub window_id: String,
}

#[function_component(ChatApp)]
pub fn chat_app(props: &ChatAppProps) -> Html {
    let messages = use_state(Vec::new);
    let input = use_state(String::new);
    let ws = use_web_socket("ws://localhost:8001/ws");
    
    let on_send = {
        let input = input.clone();
        let window_id = props.window_id.clone();
        Callback::from(move |_| {
            let msg = ChatMsg::UserTyped {
                text: (*input).clone(),
                window_id: window_id.clone(),
            };
            // Send via WebSocket
            // ws.send(serde_json::to_string(&msg).unwrap());
            input.set(String::new());
        })
    };
    
    html! {
        <div style="display: flex; flex-direction: column; height: 100%; padding: 16px;">
            <div style="flex: 1; overflow-y: auto;">
                {messages.iter().map(|msg| html! {
                    <div>{msg}</div>
                }).collect::<Html>()}
            </div>
            <div style="display: flex; gap: 8px; margin-top: 8px;">
                <input 
                    type="text" 
                    value={(*input).clone()}
                    onchange={let input = input.clone(); move |e: Event| {
                        input.set(e.target_unchecked_into::<HtmlInputElement>().value());
                    }}
                    style="flex: 1;"
                />
                <button onclick={on_send}>{"Send"}</button>
            </div>
        </div>
    }
}
```

---

## Phase 5: Integration & Launch (30 minutes)

### 5.1 Trunk.toml (for Yew build)

```toml
# sandbox-ui/Trunk.toml
[build]
target = "index.html"

dist = "dist"

[serve]
port = 5173

[watch]
watch = ["src", "../shared-types/src"]
```

### 5.2 Single Binary Build

**Development (local):**
```bash
# Build Yew UI
cd sandbox-ui
trunk build --release

# Copy static files into sandbox (embedded in binary)
cp -r dist/* ../sandbox/static/

# Build single binary (includes both Actix API + Yew static files)
cd ..
cargo build --release --package sandbox

# Run locally
./target/release/sandbox
# Serves on localhost:8080
# - /api/* → API routes
# - /ws → WebSocket
# - /* → Yew app (index.html + WASM)
```

**Production (Docker - Single Container):**

```dockerfile
# sandbox/Dockerfile
FROM rust:1.75 as builder

# Build Yew UI
RUN cargo install trunk
WORKDIR /app/sandbox-ui
COPY sandbox-ui .
RUN trunk build --release

# Build Actix binary with embedded static files
WORKDIR /app
COPY . .
RUN cp -r sandbox-ui/dist/* sandbox/static/
RUN cargo build --release --package sandbox

# Runtime image
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y sqlite3 ca-certificates
COPY --from=builder /app/target/release/sandbox /usr/local/bin/sandbox
COPY --from=builder /app/sandbox/static /app/static

# Environment
ENV CHOIR_DATA_DIR=/data
ENV CHOIR_STATIC_DIR=/app/static
EXPOSE 8080

CMD ["sandbox"]
```

**Why single binary:**
- One container per user = simple deployment
- No CORS issues (same origin)
- Static files served by Actix (no nginx needed)
- Easy to spawn/kill containers

**Docker Compose (local multi-tenant testing):**
```yaml
version: '3'
services:
  hypervisor:
    build: ./hypervisor
    ports:
      - "8001:8001"
    environment:
      - CHOIR_ENV=development
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock  # To spawn sandboxes

  # Template image for sandboxes
  sandbox-template:
    build: ./sandbox
    image: choir-sandbox:latest
    # Not run, just built as template
```

**Deployment flow:**
1. Build sandbox image (single binary with UI)
2. Hypervisor spawns container per user
3. Each container gets port mapping
4. Hypervisor routes WebSocket to correct port

---

## Time Breakdown - Chat First MVP (6 Hours Total)

**Phase 1: Foundation (2 hours)**
| Task | Time |
|------|------|
| Workspace + Justfile + deps | 30 min |
| Shared types (Event, ChatMsg) | 30 min |
| Event store actor (SQLite) | 30 min |
| Tracing/logging setup | 30 min |

**Phase 2: Chat Backend (1.5 hours)**
| Task | Time |
|------|------|
| Chat actor | 30 min |
| Basic tools (bash, read_file) | 30 min |
| API handlers (/chat/send, /chat/stream) | 30 min |

**Phase 3: Chat Frontend (1.5 hours)**
| Task | Time |
|------|------|
| Yew setup + Trunk | 30 min |
| Chat UI component | 30 min |
| WebSocket integration | 30 min |

**Phase 4: Single Binary + Docker (1 hour)**
| Task | Time |
|------|------|
| Static file serving | 30 min |
| Dockerfile | 15 min |
| Docker Compose | 15 min |

**Total: 6 hours for working chat with tool calls**

**After MVP (future work):**
- Add Writer app (use chat to create docs)
- Full desktop with windows
- Other apps (mail, calendar)

---

## Key Decisions That Make This Fast

1. **No Python at all** - Pure Rust, single toolchain
2. **Shared types** - No API schema mismatches
3. **SQLite** - No external DB to configure
4. **Event sourcing** - No complex migrations, just append
5. **Yew hooks** - Like React, familiar patterns
6. **Actor model** - Natural fit for desktop apps
7. **Single binary per sandbox** - Easy to deploy
8. **Greenfield** - No legacy compatibility burden

---

## Next Steps After MVP

1. **Add more apps** - Mail, Calendar (copy-paste pattern)
2. **BAML integration** - LLM actor for code generation
3. **Self-modification** - Prompt → generate Yew component → hot reload
4. **Multi-tenant** - Hypervisor routing multiple sandboxes
5. **Sprites integration** - Run sandboxes in containers

**Ready to start the 6-hour sprint?**