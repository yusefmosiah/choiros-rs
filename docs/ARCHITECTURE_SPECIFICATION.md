# ChoirOS Architecture Specification v1.0

**Mission:** Build the "Automatic Computer" - a self-modifying, multi-tenant system where users prompt the computer to build new programs.

**Core Philosophy:**
- State lives in actors (SQLite), UI is a reactive projection
- One sandbox per user = complete isolated computer
- Hypervisor routes, sandboxes compute
- Event sourcing enables time travel and auditability

---

## Table of Contents

1. [System Overview](#1-system-overview)
2. [Architecture Diagram](#2-architecture-diagram)
3. [Component Specifications](#3-component-specifications)
4. [Data Flow](#4-data-flow)
5. [API Contracts](#5-api-contracts)
6. [Event Contract](#6-event-contract)
7. [Deployment Architecture](#7-deployment-architecture)
8. [Development Workflow](#8-development-workflow)
9. [Testing Strategy](#9-testing-strategy)
10. [CI/CD Pipeline](#10-cicd-pipeline)
11. [Observability](#11-observability)
12. [Security Model](#12-security-model)
13. [Open Questions](#13-open-questions)

---

## 1. System Overview

### 1.1 What We're Building

A web-based "automatic computer" where:
- Each user gets their own isolated "computer" (sandbox)
- Users interact via a web desktop with multiple apps (chat, writer, etc.)
- Users can prompt the system to build new functionality
- The system compiles and hot-swaps code while preserving state
- Everything is event-sourced and auditable

### 1.2 Key Principles

1. **Actor-owned state** - State lives in SQLite, actors query their own state
2. **UI is a projection** - UI components read from actors, never own state
3. **Optimistic updates** - UI updates immediately, confirms async with actor
4. **Event sourcing** - All changes logged, enables replay and audit
5. **Hot reload** - UI components can be swapped at runtime without losing state
6. **Sandbox isolation** - One container per user, complete capability boundary

### 1.3 Technology Stack

| Layer | Technology | Purpose |
|-------|-----------|---------|
| Frontend | Dioxus (WASM) | Reactive UI with signals |
| Backend | Actix (Actors) | Async actor system |
| Database | SQLite | Event log + projections |
| LLM | BAML + Bedrock | Code generation |
| Sandbox | Sprites.dev | Container per user |
| Hypervisor | Actix Web | Edge routing |
| Protocol | WebSocket + HTTP | Real-time communication |

---

## 2. Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              USER BROWSER                                    │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │ Dioxus WASM Application                                               │  │
│  │                                                                       │  │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐       │  │
│  │  │ ChatApp         │  │ WriterApp       │  │ UserApp (hot)   │       │  │
│  │  │ Component       │  │ Component       │  │ swap            │       │  │
│  │  │                 │  │                 │  │                 │       │  │
│  │  │ use_resource()  │  │ use_resource()  │  │ use_resource()  │       │  │
│  │  │   ↓             │  │   ↓             │  │   ↓             │       │  │
│  │  │ Query Actor     │  │ Query Actor     │  │ Query Actor     │       │  │
│  │  └────────┬────────┘  └────────┬────────┘  └────────┬────────┘       │  │
│  │           │                    │                    │                │  │
│  │           └────────────────────┴────────────────────┘                │  │
│  │                              │                                        │  │
│  │                    WebSocket / HTTP                                  │  │
│  └──────────────────────────────┼────────────────────────────────────────┘  │
└──────────────────────────────────┼──────────────────────────────────────────┘
                                   │
                        ┌──────────┴──────────┐
                        │  HYPERVISOR (Port   │
                        │       8001)         │
                        │                     │
                        │ • WebAuthn/Passkey  │
                        │ • Route to sandbox  │
                        │ • Spawn/kill        │
                        │ • No business logic │
                        │ • No user data      │
                        └──────────┬──────────┘
                                   │
         ┌─────────────────────────┼─────────────────────────┐
         │                         │                         │
         ▼                         ▼                         ▼
┌─────────────────┐   ┌─────────────────┐   ┌─────────────────┐
│  USER A SANDBOX │   │  USER B SANDBOX │   │  USER C SANDBOX │
│  (Port 9001)    │   │  (Port 9002)    │   │  (Port 9003)    │
│                 │   │                 │   │                 │
│ ┌─────────────┐ │   │ ┌─────────────┐ │   │ ┌─────────────┐ │
│ │ Actix Web   │ │   │ │ Actix Web   │ │   │ │ Actix Web   │ │
│ │ Server      │ │   │ │ Server      │ │   │ │ Server      │ │
│ │             │ │   │ │             │ │   │ │             │ │
│ │ • /api/*    │ │   │ │ • /api/*    │ │   │ │ • /api/*    │ │
│ │ • /ws       │ │   │ │ • /ws       │ │   │ │ • /ws       │ │
│ │ • /static   │ │   │ │ • /static   │ │   │ │ • /static   │ │
│ └──────┬──────┘ │   │ └──────┬──────┘ │   │ └──────┬──────┘ │
│        │        │   │        │        │   │        │        │
│ ┌──────▼──────┐ │   │ ┌──────▼──────┐ │   │ ┌──────▼──────┐ │
│ │ Actor System│ │   │ │ Actor System│ │   │ │ Actor System│ │
│ │             │ │   │ │             │ │   │ │             │ │
│ │ • ChatActor │ │   │ │ • ChatActor │ │   │ │ • ChatActor │ │
│ │ • WriterActr│ │   │ │ • WriterActr│ │   │ │ • WriterActr│ │
│ │ • EventStore│ │   │ │ • EventStore│ │   │ │ • EventStore│ │
│ │ • BAML      │ │   │ │ • BAML      │ │   │ │ • BAML      │ │
│ └──────┬──────┘ │   │ └──────┬──────┘ │   │ └──────┬──────┘ │
│        │        │   │        │        │   │        │        │
│ ┌──────▼──────┐ │   │ ┌──────▼──────┐ │   │ ┌──────▼──────┐ │
│ │ SQLite      │ │   │ │ SQLite      │ │   │ │ SQLite      │ │
│ │             │ │   │ │             │ │   │ │             │ │
│ │ events      │ │   │ │ events      │ │   │ │ events      │ │
│ │ projections │ │   │ │ projections │ │   │ │ projections │ │
│ │ actor_state │ │   │ │ actor_state │ │   │ │ actor_state │ │
│ └─────────────┘ │   │ └─────────────┘ │   │ └─────────────┘ │
└─────────────────┘   └─────────────────┘   └─────────────────┘
```

---

## 3. Component Specifications

### 3.1 Hypervisor

**Purpose:** Stateless edge router. Authenticates users and routes to their sandbox.

**Interface:**
```rust
// src/main.rs
pub struct Hypervisor {
    sandboxes: HashMap<UserId, SandboxHandle>,
    auth: AuthProvider,
}

impl Hypervisor {
    // Route WebSocket to user's sandbox
    async fn route_websocket(&self, user_id: UserId, stream: WebSocket) -> Result<()>;
    
    // Spawn new sandbox for user
    async fn spawn_sandbox(&mut self, user_id: UserId) -> Result<SandboxHandle>;
    
    // Kill sandbox (user logout or timeout)
    async fn kill_sandbox(&mut self, user_id: UserId) -> Result<()>;
}
```

**HTTP Routes:**
- `GET /health` - Health check
- `GET /ws` - WebSocket upgrade (authenticated)
- `POST /sandbox/spawn` - Spawn new sandbox (admin only)

**No business logic. No user data. Pure routing.**

### 3.2 Sandbox

**Purpose:** Complete ChoirOS instance per user. One binary serves API + static UI.

**Interface:**
```rust
// src/main.rs
pub struct Sandbox {
    actor_system: ActorSystem,
    event_store: Addr<EventStoreActor>,
    port: u16,
}

impl Sandbox {
    async fn run(&self) -> Result<()> {
        HttpServer::new(|| {
            App::new()
                // API routes
                .service(api::chat_send)
                .service(api::chat_stream)
                .service(api::actor_query)
                // Static files (Dioxus UI)
                .service(Files::new("/", "./static").index_file("index.html"))
        })
        .bind(format!("0.0.0.0:{}", self.port))?
        .run()
        .await
    }
}
```

**Actors (all in one process):**
- `EventStoreActor` - SQLite event log
- `ChatActor` - Chat app logic
- `WriterActor` - Writer app logic  
- `BamlActor` - LLM integration
- `ToolExecutor` - Tool execution (bash, file ops)

### 3.3 EventStore Actor

**Purpose:** Append-only event log. All state changes go through here.

**Interface:**
```rust
pub struct EventStoreActor {
    pool: SqlitePool,
    subscribers: Vec<Recipient<EventPublished>>,
}

// Messages
#[derive(Message)]
#[rtype(result = "Result<Event, Error>")]
pub struct AppendEvent {
    pub event_type: String,
    pub payload: JsonValue,
    pub actor_id: String,
}

#[derive(Message)]
#[rtype(result = "Vec<Event>")]
pub struct QueryEvents {
    pub actor_id: String,
    pub since_seq: i64,
}

#[derive(Message, Clone)]
#[rtype(result = "()")]
pub struct EventPublished(pub Event);
```

**Schema:**
```sql
CREATE TABLE events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    actor_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    payload JSON NOT NULL
);

CREATE INDEX idx_events_actor ON events(actor_id);
CREATE INDEX idx_events_type ON events(event_type);
CREATE INDEX idx_events_timestamp ON events(timestamp);
```

### 3.4 ChatActor

**Purpose:** Chat application logic. Owns chat state in SQLite.

**Interface:**
```rust
pub struct ChatActor {
    actor_id: String,
    event_store: Addr<EventStoreActor>,
}

// Messages
#[derive(Message)]
#[rtype(result = "()")]
pub struct SendMessage {
    pub text: String,
    pub user_id: String,
}

#[derive(Message)]
#[rtype(result = "Vec<ChatMessage>")]
pub struct GetMessages;

#[derive(Message)]
#[rtype(result = "()")]
pub struct RunTool {
    pub tool_name: String,
    pub args: JsonValue,
}
```

**State Storage:**
```sql
CREATE TABLE chat_messages (
    id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    text TEXT NOT NULL,
    sender TEXT NOT NULL, -- 'user' | 'assistant' | 'system'
    timestamp TEXT NOT NULL,
    tool_calls JSON -- If this message triggered tools
);
```

### 3.5 Dioxus UI Components

**Purpose:** Reactive UI. Queries actors for state, never owns state.

**Pattern:**
```rust
#[component]
fn ChatView(actor_id: String) -> Element {
    // Poll actor for messages
    let messages = use_resource(move || {
        let id = actor_id.clone();
        async move {
            query_actor::<ChatActor>(&id, GetMessages).await
        }
    });
    
    // Optimistic state for immediate feedback
    let optimistic = use_signal(Vec::new);
    
    let send = move |text: String| {
        // 1. Optimistic update
        let msg = Message::new(&text);
        optimistic.push(msg.clone());
        
        // 2. Send to actor
        spawn(async move {
            let _ = send_to_actor(&actor_id, SendMessage { 
                text, 
                user_id: "me".into() 
            }).await;
        });
    };
    
    rsx! {
        for msg in messages().unwrap_or_default().iter().chain(optimistic.iter()) {
            MessageBubble { msg }
        }
        Input { on_send: send }
    }
}
```

---

## 4. Data Flow

### 4.1 User Sends Message

```
User types → Enter
    ↓
Dioxus: optimistic.set(msg) [instant UI update]
    ↓
POST /api/chat/send {actor_id, text}
    ↓
Actix Handler
    ↓
ChatActor.handle(SendMessage)
    ↓
EventStoreActor.append("chat.user_msg", payload)
    ↓
SQLite INSERT
    ↓
Event broadcast to subscribers
    ↓
ChatActor (projection update)
    ↓
WebSocket push to client
    ↓
Dioxus use_resource refreshes
    ↓
UI updates (optimistic msg now confirmed)
```

### 4.2 Tool Execution

```
ChatActor detects "/tool" command
    ↓
Parse tool name and args
    ↓
ToolExecutor.execute(tool_name, args)
    ↓
EventStore.append("tool.call", {tool, args})
    ↓
Execute (bash, file op, etc.)
    ↓
EventStore.append("tool.result", {output})
    ↓
ChatActor receives result
    ↓
Maybe trigger LLM response
    ↓
WebSocket push
```

### 4.3 Hot Reload UI

```
User prompts: "Make chat look like iMessage"
    ↓
BAML generates new component code
    ↓
Compile to WASM: wasm32-unknown-unknown
    ↓
Sandbox.hot_swap_component("ChatApp", wasm_bytes)
    ↓
Dioxus: Replace component in registry
    ↓
Re-render with same actor_id
    ↓
State preserved (it's in SQLite, not component)
```

---

## 5. API Contracts

### 5.1 REST API (Sandbox)

**Chat Endpoints:**
```
POST /api/chat/send
Request: {actor_id: string, text: string}
Response: {message_id: string, status: "queued"}

GET /api/chat/messages?actor_id={id}&since={seq}
Response: {messages: [{id, text, sender, timestamp}], last_seq: number}

POST /api/chat/tool
Request: {actor_id: string, tool: string, args: object}
Response: {result: object, status: "success" | "error"}
```

**Actor Query:**
```
POST /api/actor/query
Request: {actor_id: string, query_type: string, params: object}
Response: {data: object}
```

### 5.2 WebSocket Protocol

**Connection:**
```
Client: GET /ws (with JWT in header)
Server: 101 Switching Protocols
```

**Messages (JSON):**
```typescript
// Client → Server
type ClientMsg = 
  | { type: "subscribe", actor_id: string }
  | { type: "send", actor_id: string, payload: object }
  | { type: "query", actor_id: string, query: string };

// Server → Client  
type ServerMsg =
  | { type: "event", actor_id: string, event: Event }
  | { type: "state", actor_id: string, state: object }
  | { type: "error", message: string };
```

---

## 6. Event Contract

### 6.1 Event Types

All events follow this schema:
```rust
pub struct Event {
    pub seq: i64,              // Global ordering
    pub event_id: String,      // ULID
    pub timestamp: DateTime<Utc>,
    pub actor_id: String,      // Which actor produced this
    pub event_type: String,    // Domain.event format
    pub payload: JsonValue,    // Event-specific data
    pub user_id: String,       // Who triggered this
}
```

### 6.2 Core Events

**Chat Domain:**
```rust
// chat.user_msg
{
    "text": "Hello world",
    "window_id": "chat-1"
}

// chat.assistant_msg  
{
    "text": "How can I help?",
    "model": "claude-3-sonnet",
    "thinking": "..."
}

// chat.tool_call
{
    "tool": "bash",
    "args": {"command": "ls -la"},
    "call_id": "call-123"
}

// chat.tool_result
{
    "call_id": "call-123",
    "status": "success",
    "output": "..."
}
```

**File Domain:**
```rust
// file.write
{
    "path": "hello.txt",
    "content_hash": "sha256:abc...",
    "size": 123
}

// file.edit
{
    "path": "hello.txt",
    "old_string": "Hello",
    "new_string": "Hi"
}
```

**System Domain:**
```rust
// actor.spawned
{
    "actor_type": "ChatActor",
    "actor_id": "chat-1"
}

// actor.hot_swap
{
    "actor_id": "chat-1",
    "old_component": "ChatApp-v1",
    "new_component": "ChatApp-v2"
}
```

---

## 7. Deployment Architecture

### 7.1 Production (AWS)

```
┌─────────────────────────────────────────────────────────┐
│                      AWS EC2                             │
│  ┌─────────────────────────────────────────────────────┐│
│  │ Docker Host                                         ││
│  │                                                     ││
│  │  ┌──────────────┐   ┌────────────────────────────┐ ││
│  │  │ Hypervisor   │   │  User Sandboxes            │ ││
│  │  │ Container    │   │  ┌────────┐ ┌────────┐     │ ││
│  │  │              │   │  │User A  │ │User B  │     │ ││
│  │  │ • Port 8001  │◄──┼──│(Port   │ │(Port   │     │ ││
│  │  │ • Routes WS  │   │  │ 9001)  │ │ 9002)  │     │ ││
│  │  │ • Auth       │   │  └────────┘ └────────┘     │ ││
│  │  └──────────────┘   └────────────────────────────┘ ││
│  │                                                     ││
│  │  ┌──────────────┐                                  ││
│  │  │ Sprites      │ - Container runtime              ││
│  │  │ Adapter      │ - Spawns/kills sandboxes         ││
│  │  └──────────────┘                                  ││
│  └─────────────────────────────────────────────────────┘│
│                                                          │
│  ┌─────────────────────────────────────────────────────┐│
│  │ EBS Volume (per user)                               ││
│  │ - SQLite persistence                                ││
│  │ - Git repository                                    ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
```

### 7.2 Docker Compose (Local)

```yaml
# docker-compose.yml
version: '3.8'
services:
  hypervisor:
    build: ./hypervisor
    ports:
      - "8001:8001"
    environment:
      - CHOIR_ENV=development
      - SPRITES_API_TOKEN=${SPRITES_API_TOKEN}
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - ./data:/data
    depends_on:
      - sprites-adapter

  sprites-adapter:
    build: ./sprites-adapter
    environment:
      - SPRITES_API_TOKEN=${SPRITES_API_TOKEN}
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock

  sandbox-template:
    build: ./sandbox
    image: choir-sandbox:latest
    # Not run, just built as template

volumes:
  data:
```

### 7.3 Environment Variables

**Hypervisor:**
```bash
CHOIR_ENV=development|production
CHOIR_HYPERVISOR_PORT=8001
CHOIR_SANDBOX_PORT_RANGE=9000-9999
SPRITES_API_TOKEN=xxx
```

**Sandbox:**
```bash
CHOIR_DATA_DIR=/data
CHOIR_ACTOR_ID={user-specific}
CHOIR_BAML_PROVIDER=bedrock|zai
CHOIR_BAML_PROVIDER=bedrock  # or zai
# Model specified in BAML files, not env
```

---

## 8. Development Workflow

### 8.1 Local Development

```bash
# Terminal 1: Start hypervisor
cd hypervisor
cargo run

# Terminal 2: Start sandbox
cd sandbox
cargo run

# Terminal 3: Dioxus dev server
cd sandbox-ui
dx serve --hot-reload

# Access: http://localhost:5173
```

### 8.2 Building for Production

```bash
# Build UI
cd sandbox-ui
dx build --release

# Copy to sandbox static
cp -r dist/* ../sandbox/static/

# Build sandbox binary
cd ../sandbox
cargo build --release

# Build Docker image
docker build -t choir-sandbox:latest .
```

### 8.3 Testing Locally

```bash
# Run all tests
cargo test --workspace

# Run specific test
cargo test chat_actor --package sandbox

# Integration tests
cargo test --test integration
```

---

## 9. Testing Strategy

### 9.1 Unit Tests

**Actor Tests:**
```rust
#[actix::test]
async fn test_chat_actor_send_message() {
    let event_store = EventStoreActor::new_in_memory().start();
    let chat = ChatActor::new("test-chat", event_store).start();
    
    // Send message
    chat.send(SendMessage {
        text: "Hello".into(),
        user_id: "user-1".into(),
    }).await.unwrap();
    
    // Query messages
    let msgs = chat.send(GetMessages).await.unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].text, "Hello");
}
```

**Event Store Tests:**
```rust
#[actix::test]
async fn test_event_append_and_query() {
    let store = EventStoreActor::new_in_memory().await.unwrap().start();
    
    // Append event
    let event = store.send(AppendEvent {
        event_type: "test.event".into(),
        payload: json!({"foo": "bar"}),
        actor_id: "actor-1".into(),
        user_id: "test".into(),
    }).await.unwrap().unwrap();
    
    assert!(event.seq > 0);
    
    // Query events
    let events = store.send(GetEventsForActor {
        actor_id: "actor-1".into(),
        since_seq: 0,
    }).await.unwrap().unwrap();
    
    assert_eq!(events.len(), 1);
}
```

### 9.2 Integration Tests

**Full Flow Test:**
```rust
#[tokio::test]
async fn test_chat_full_flow() {
    // Spawn sandbox
    let sandbox = spawn_test_sandbox().await;
    
    // Connect Dioxus (simulated)
    let client = TestClient::connect(&sandbox.ws_url()).await;
    
    // Send message
    client.send_message("Hello").await;
    
    // Wait for response
    let messages = client.wait_for_messages(Duration::from_secs(5)).await;
    assert!(messages.len() >= 1);
}
```

### 9.3 E2E Tests

Using Playwright or similar:
```typescript
// tests/chat.spec.ts
test('user can send message', async ({ page }) => {
  await page.goto('http://localhost:5173');
  
  // Type message
  await page.fill('[data-testid="chat-input"]', 'Hello');
  await page.click('[data-testid="send-button"]');
  
  // Verify appears in UI
  await expect(page.locator('.message')).toContainText('Hello');
});
```

---

## 10. CI/CD Pipeline

### 10.1 GitHub Actions Workflow

```yaml
# .github/workflows/ci.yml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
          components: rustfmt, clippy
      
      - name: Install cargo tools
        run: |
          cargo install cargo-nextest
          cargo install dioxus-cli
      
      - name: Cache dependencies
        uses: Swatinem/rust-cache@v2
      
      - name: Check formatting
        run: cargo fmt -- --check
      
      - name: Run clippy
        run: cargo clippy --workspace -- -D warnings
      
      - name: Run tests
        run: cargo nextest run --workspace
      
      - name: Build sandbox
        run: |
          cd sandbox-ui && dx build --release
          cp -r dist/* ../sandbox/static/
          cd ../sandbox && cargo build --release
      
      - name: Build Docker image
        run: docker build -t choir-sandbox:${{ github.sha }} ./sandbox

  deploy:
    needs: test
    runs-on: ubuntu-latest
    if: github.ref == 'refs/heads/main'
    steps:
      - name: Deploy to EC2
        run: |
          # SSH to EC2 and pull new image
          ssh ubuntu@3.83.131.245 "docker pull choir-sandbox:${{ github.sha }}"
          ssh ubuntu@3.83.131.245 "docker-compose up -d"
```

---

## 11. Observability

### 11.1 Structured Logging

Using `tracing` with JSON output:
```rust
use tracing::{info, debug, error, instrument};

#[instrument(skip(self, msg))]
async fn handle_send_message(&self, msg: SendMessage) {
    debug!(actor_id = %self.actor_id, text = %msg.text, "Sending message");
    
    match self.event_store.send(append_event).await {
        Ok(event) => {
            info!(event_id = %event.event_id, seq = event.seq, "Event appended");
        }
        Err(e) => {
            error!(error = %e, "Failed to append event");
        }
    }
}
```

**Log Levels:**
- `ERROR` - Failures requiring intervention
- `WARN` - Degraded service but functioning  
- `INFO` - Key events (actor spawn, user actions)
- `DEBUG` - Detailed flow (event processing)
- `TRACE` - Verbose (HTTP request details)

### 11.2 Metrics

**Prometheus metrics endpoint:**
```
GET /metrics

# HELP choir_events_total Total events appended
# TYPE choir_events_total counter
choir_events_total{actor_type="ChatActor"} 1234

# HELP choir_actor_count Number of active actors
# TYPE choir_actor_count gauge  
choir_actor_count 5

# HELP choir_message_latency_seconds Message processing latency
# TYPE choir_message_latency_seconds histogram
choir_message_latency_seconds_bucket{le="0.1"} 100
```

### 11.3 Health Checks

```rust
async fn health_check() -> HttpResponse {
    // Simple health check - in production, verify database connectivity
    // and actor system status here
    HttpResponse::Ok().json(json!({
        "status": "healthy",
        "checks": {
            "database": "ok",
            "actors": "ok"
        }
    }))
}
```

### 11.4 Tracing

Distributed tracing across components:
```rust
// Hypervisor → Sandbox
let span = tracing::info_span!("route_request", user_id = %user_id);
let _enter = span.enter();

// Carries trace ID through WebSocket
// View in Jaeger or similar
```

---

## 12. Security Model

### 12.1 Authentication

**WebAuthn / Passkey:**
```rust
// User registers passkey
// Private key on device, public key stored in hypervisor

// Authentication flow:
// 1. Client requests challenge
// 2. Server generates challenge, stores in session
// 3. Client signs with private key
// 4. Server verifies with public key
// 5. Issue JWT for WebSocket
```

### 12.2 Authorization

**Capability-based:**
```rust
pub struct Membrane {
    actor_id: String,
    can_message: Vec<ActorId>,      // Which actors can receive msgs
    can_read_events: Vec<String>,   // Which event types
    can_write_events: Vec<String>,  // Which event types  
    filesystem_scope: Option<PathBuf>, // Scoped to project
}

// Chat actor can:
// - Read/write chat.* events
// - Message writer actor
// - No filesystem access

// Writer actor can:
// - Read/write file.* events
// - Read chat.* events (for context)
// - Access ./docs/* only
```

### 12.3 Sandbox Isolation

- **Network:** Sandboxes can only call out (LLM APIs), not accept incoming
- **Filesystem:** Scoped to user volume, no host access
- **Process:** No privileged operations, no exec of arbitrary binaries
- **Memory:** Resource limits enforced by container runtime

### 12.4 Secrets Management

```bash
# .env (never committed)
SPRITES_API_TOKEN=xxx
AWS_ACCESS_KEY_ID=xxx
AWS_SECRET_ACCESS_KEY=xxx

# BAML credentials configured via BAML files, not env
# Provider (Bedrock/Z.ai) and models set in .baml files

# In production, use:
# - AWS Secrets Manager
# - Docker secrets
# - Or mount from secure volume
```

### 12.5 Offline Strategy (MVP)

**MVP: Online-Only**
- Require network connection to sandbox
- Show "Reconnecting..." when WebSocket drops
- On page reload: fetch fresh state from actor
- No localStorage caching (KISS principle)

**Post-MVP: Graceful Degradation**
```rust
// Future enhancement: LocalStorage backup for drafts
fn EditorComponent(actor_id: String) -> Element {
    let draft_backup = use_local_storage::<String>("draft_backup");
    
    // On network error: show cached content with sync status
    // On reconnect: sync to server
    // User sees: "Last synced 2 min ago" with manual refresh
}
```

**File Browser:**
- **MVP**: Always fetch fresh from actor (simplest, no stale data issues)
- **Future**: Cache directory listing locally with sync indicator

---

## 13. BAML Integration (Rust)

### 13.1 Crate Availability

The `baml = "0.218.0"` crate provides:
- ✅ `AsyncStreamingCall` - Async streaming with partial results
- ✅ `ClientRegistry` - Runtime client switching (Bedrock ↔ Z.ai)
- ✅ `StreamingCall` / `LLMStreamCall` - Full streaming support
- ✅ BAML docs confirm AWS Bedrock `ConverseStream` endpoint support

**No Python bridge needed!**

### 13.2 Usage Pattern

```rust
use baml::{BamlRuntime, ClientRegistry};

pub struct BamlActor {
    runtime: BamlRuntime,
    clients: ClientRegistry,
}

impl BamlActor {
    pub fn new() -> Self {
        let runtime = BamlRuntime::new();
        let clients = ClientRegistry::new();
        
        // Register Bedrock client
        clients.add_client("Bedrock", bedrock_config());
        
        Self { runtime, clients }
    }

    pub async fn stream_response(
        &self,
        messages: Vec<Message>,
    ) -> impl Stream<Item = StreamChunk> {
        let client = self.clients.get("Bedrock").unwrap();
        
        self.runtime
            .with_client(client)
            .stream_chat(messages)
            .await
    }
}
```

### 13.3 BAML Files

Project structure:
```
sandbox/
├── src/
│   └── actors/
│       └── baml.rs
└── baml/
    ├── clients.baml      # Bedrock, Z.ai configurations
    ├── functions.baml    # PlanAction, Chat, etc.
    └── types.baml        # Message, ToolCall, etc.
```

**Note:** Models specified in BAML files, not environment variables.
Current models (from your Python setup):
- AWS Bedrock: anthropic.claude-3-opus-4.5 (via BAML client)

---

## 14. Open Questions

### 13.1 Technical Decisions

| Question | Options | Status |
|----------|---------|--------|
| Dioxus vs Yew? | Dioxus (better dev experience) | **Decided: Dioxus** |
| Hot reload mechanism? | WASM swap vs iframe | **TBD: Start with WASM** |
| State sync protocol? | Polling vs WebSocket push | **TBD: WebSocket for real-time** |
| Actor supervision? | Actix Supervisor vs custom | **TBD: Actix Supervisor** |
| BAML in Rust? | Native crate vs Python bridge | **TBD: Try native first** |

### 13.2 Scope Questions

1. **Do we migrate existing data?**
   - Option A: Fresh start (new event log)
   - Option B: Import from Python SQLite
   - **Decision needed:** Start fresh, migration later

2. **Which apps to build first?**
   - MVP: Chat only
   - Phase 2: Chat + Writer
   - Phase 3: Add Mail, Calendar
   - **Decision needed:** Chat first, then Writer

3. **Multi-user features?**
   - MVP: Single user per sandbox
   - Future: Shared workspaces
   - **Decision needed:** Single user for now

### 13.3 Research Needed

1. **Dioxus WASM module loading** - How to dynamically load/swap components?
2. **BAML Rust integration** - Does the native crate support all features?
3. **Sprites.dev performance** - Startup time, resource limits
4. **WebAuthn implementation** - Library support in Rust

---

## Quick Start for AI Agents

If you're an AI agent reading this to implement the system:

1. **Start here:** `shared-types/` - Define Event, ActorId, all messages
2. **Then:** `sandbox/src/event_store.rs` - SQLite append-only log
3. **Then:** `sandbox/src/actors/chat.rs` - Chat actor
4. **Then:** `sandbox-ui/src/chat.rs` - Dioxus chat component
5. **Finally:** Wire together with HTTP handlers

**Key constraints:**
- Never put state in UI components
- Always go through EventStore
- Use optimistic updates for UI responsiveness
- Test with `cargo nextest run`

---

## Changelog

- **v1.0** - Initial architecture specification
  - Single binary per sandbox
  - Actor-owned state with SQLite
  - Dioxus frontend with optimistic updates
  - Hot reload architecture
  - Chat-first MVP

---

**Next Steps:**
1. Create `choiros-rs` repository
2. Implement shared types
3. Build event store
4. Create chat actor + UI
5. Test end-to-end

**Ready to build.**