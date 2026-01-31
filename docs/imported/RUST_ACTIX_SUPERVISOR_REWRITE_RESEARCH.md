# Rust Actix Supervisor Rewrite: Comprehensive Research Report

**Date**: 2026-01-30
**Status**: Research Complete
**Goal**: Document all requirements for migrating Python Supervisor to Rust Actix

---

## Executive Summary

The ChoirOS Supervisor is a complex event-sourced system that orchestrates AI agents using:
- **Ray actors** for concurrent execution
- **FastAPI** for HTTP/WebSocket/SSE endpoints
- **SQLite/libsql** for event storage and projections
- **BAML** for LLM integration (Anthropic/Bedrock)
- **Asyncio** for Python concurrency

A Rust Actix rewrite targets:
- Type safety and compile-time guarantees
- Performance improvements (async/actor model)
- Eliminate Ray dependency overhead
- Single binary deployment
- Better observability

**Key Finding**: The rewrite is feasible but requires careful handling of:
1. Ray actor model → Rust tokio channels or dedicated actor framework
2. Python dependencies (BAML, sandbox adapters) → HTTP bridge or pyo3 bindings
3. Event contract compatibility → Preserve 87 event types exactly
4. Frontend WebSocket/SSE protocols → Exact protocol preservation
5. SQLite migration → Zero-loss cutover strategy

**Effort Estimate**: 4-6 months for full feature parity

---

## 1. Current Architecture Analysis

### 1.1 Core Components

```
supervisor/
├── main.py                    (1146 lines) - FastAPI app + WebSocket handler
├── ray_bus.py                (123 lines)  - Ray event bus (pub/sub)
├── event_publisher.py          (199 lines)  - Event publishing to bus
├── event_model.py             (29 lines)   - ChoirEvent dataclass
├── event_contract.py           (124 lines)  - 87 event types
├── db.py                     (1300+ lines) - ProjectionStore + SQLite schema
├── runtime_store.py           (141 lines)   - Event dedupe tracking
├── machine.py                 (357 lines)   - Mode orchestration
├── run_orchestrator.py       (544 lines)   - CALM→VERIFY→SKEPTICAL flow
├── agent/
│   ├── harness.py             (278 lines)   - BAML agent loop
│   └── tools.py              - Tool implementations
├── sandbox_runner.py          (253 lines)   - Sandbox lifecycle
├── sandbox_provider.py        (17 lines)    - Provider selection
├── verifier_runner.py          (249 lines)   - Verification execution
├── verifier_plan.py           (161 lines)   - Verifier selection
├── provider_factory.py         (188 lines)   - BAML provider (Bedrock/Z.ai)
├── baml_client/              - Generated LLM clients
├── git_ops.py               (365 lines)   - Git checkpointing
├── auditor_worker.py          (106 lines)   - Ray worker: file audit
└── projector_worker.py         (90 lines)   - Ray worker: projection updates
```

### 1.2 Data Flow Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Frontend (React + Vite)                 │
│  - WebSocket: /agent (bi-directional streaming)            │
│  - SSE: /events/stream (event streaming)                   │
│  - REST: work_items, runs, sandbox, git, etc.         │
└───────────────────────────┬─────────────────────────────────────┘
                        │ HTTP/WebSocket
                        ▼
┌─────────────────────────────────────────────────────────────────────┐
│              Python Supervisor (FastAPI, port 8001)          │
├─────────────────────────────────────────────────────────────────────┤
│  main.py: Request handling + WebSocket connection            │
│  ├─→ Machine: Mode selection (CALM/CURIOUS/...)      │
│  ├─→ AgentHarness: BAML agent execution                 │
│  ├─→ RunOrchestrator: Verify → SKEPTICAL → rollback  │
│  └─→ EventPublisher: Publish to Ray + SQLite          │
├─────────────────────────────────────────────────────────────────────┤
│  Ray Cluster (in-process, single node)                      │
│  ├─ EventBus: Pub/sub for events                        │
│  ├─ AuditorWorker: Subscribes to file.write events      │
│  └─ ProjectorWorker: Subscribes to * events             │
├─────────────────────────────────────────────────────────────────────┤
│  Services (Synchronous/Async)                              │
│  ├─ SQLite: Event log + materialized projections          │
│  ├─ BAML Client: LLM calls (Bedrock/Z.ai)              │
│  ├─ Sandbox: Local or Sprites.dev container               │
│  └─ Git: Checkpoints, revert, diff                         │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.3 Event Contract

**87 Canonical Event Types** (supervisor/event_contract.py:17-86)

```python
# Core events
"file.write", "file.delete", "file.move",
"message", "tool.call", "tool.result",
"window.open", "window.close",
"checkpoint", "undo",
"mode.start", "mode.stop", "mode.update", "mode.heartbeat",
"run.input", "run.started", "run.finished",
"artifact.create", "artifact.pointer",

# Document events
"document.create", "document.edit", "document.snapshot", "document.restore",
"ai.suggestion", "ai.suggestion.accept", "ai.suggestion.reject",

# Notes (AHDB telemetry)
"note.observation", "note.hypothesis", "note.hyperthesis", "note.conjecture",
"note.status", "note.request.help", "note.request.verify",

# Receipts (40+ types)
"receipt.read", "receipt.patch", "receipt.verifier", "receipt.net", "receipt.db",
"receipt.export", "receipt.publish", "receipt.context.footprint",
"receipt.verifier.results", "receipt.verifier.attestations",
"receipt.dlq", "receipt.commit", "receipt.mode.transition",
"receipt.ahdb.delta", "receipt.evidence.set.hash", "receipt.retrieval",
"receipt.conjecture.set", "receipt.policy.decision.tokens",
"receipt.security.attestations", "receipt.hyperthesis.delta",
"receipt.expansion.plan", "receipt.projection.rebuild", "receipt.attack.report",
"receipt.disclosure.objects", "receipt.mitigation.proposals", "receipt.preference.decision",
"receipt.timeout",

# Settings
"provider.changed", "provider.test"
```

**Subject Format**: `choiros.{user_id}.{source}.{event_type}`
**Sources**: `user`, `agent`, `system`

---

## 2. Rust/Actix Technology Stack

### 2.1 Recommended Crates

```toml
[dependencies]
# Web Framework
actix-web = "4.9"          # HTTP, WebSocket, SSE
actix-cors = "0.7"         # CORS middleware
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Async Runtime
tokio = { version = "1.40", features = ["full"] }

# Database
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "chrono"] }
sqlx-cli = { version = "0.8", features = ["sqlite"] }

# Python Interop (for BAML)
pyo3 = "0.23"              # Python bindings OR
reqwest = { version = "0.12", features = ["json"] }  # HTTP to Python BAML server

# Utilities
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.11", features = ["v4", "serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Testing
actix-rt = "2.10"           # Actix test runtime
tokio-test = "0.4"

# Ray Alternative: Actor Framework
# Option A: tokio channels (built-in, no extra crate)
# Option B: ractor = "0.13" (dedicated actor framework)
# Option C: xtra = "0.2" (actor library)
```

### 2.2 Key Pattern Translations

#### Python FastAPI → Rust Actix

| Python (FastAPI) | Rust (Actix) | Example |
|-------------------|----------------|---------|
| `@app.get("/path")` | `HttpServer::new(|| App::new().service(endpoint))` | See Actix docs |
| `@app.websocket("/agent")` | `web::resource("/agent").route(web::get().to(ws_handler))` | WebSocket with extractors |
| `StreamingResponse(..., media_type="text/event-stream")` | HttpResponse::Ok().content_type("text/event-stream").body(...)` | SSE manually |
| `async def handler(...)` | `async fn handler(...) -> impl Responder` | fn signatures |
| `Request: Pydantic model` | `Json<T>` extractor | serde deserialize |
| HTTPException(status_code=...)` | `ErrorHttpResponse::from(...)` | Actix error handling |

#### Python Asyncio → Rust Tokio

| Python (asyncio) | Rust (Tokio) | Example |
|-------------------|----------------|---------|
| `async def ...` | `async fn ...` | `#[tokio::main]` |
| `await asyncio.sleep(1)` | `tokio::time::sleep(Duration::from_secs(1)).await` | `tokio::time::sleep` |
| `asyncio.create_task(...)` | `tokio::spawn(async { ... })` | `tokio::spawn` |
| `asyncio.Queue()` | `tokio::sync::mpsc::channel(100)` | `tokio::sync::mpsc` |
| `asyncio.Lock()` | `tokio::sync::Mutex::new(...)` | `tokio::sync::Mutex` |

#### Python Ray → Rust Actors

**Approach A: Tokio Channels (Recommended for MVP)**

```rust
use tokio::sync::mpsc;

#[derive(Clone)]
struct EventBus {
    sender: mpsc::UnboundedSender<Event>,
}

impl EventBus {
    pub async fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    pub async fn subscribe(&self) -> mpsc::UnboundedReceiver<Event> {
        // Clone sender for each subscriber
        let (tx, rx) = mpsc::unbounded_channel();
        // TODO: Add to subscribers map
        rx
    }
}
```

**Approach B: Ractor Framework (More structured)**

```rust
use ractor::{Actor, ActorProcessingErr, ActorRef};

pub struct EventBusActor;

#[async_trait]
impl Actor for EventBusActor {
    type Msg = Event;
    type State = ();

    async fn pre_start(&self, myself: ActorRef<Self>) -> Result<(), ActorProcessingErr> {
        Ok(())
    }

    async fn handle(&self, _myself: ActorRef<Self>, message: Event, _state: &mut Self::State) -> Result<(), ActorProcessingErr> {
        // Route message to subscribers
        Ok(())
    }
}
```

#### Python SQLite → Rust SQLX

```rust
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use chrono::{DateTime, Utc};

#[derive(Debug, sqlx::FromRow)]
pub struct EventRow {
    pub seq: i64,
    pub nats_seq: Option<i64>,
    pub event_id: String,
    pub timestamp: String,
    pub event_type: String,
    pub payload: String,  // JSON stored as TEXT
}

async fn insert_event(pool: &SqlitePool, event: &ChoirEvent) -> Result<i64, sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO events (nats_seq, event_id, type, payload, timestamp)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#
    )
    .bind(event.nats_seq)
    .bind(&event.event_id)
    .bind(&event.event_type)
    .bind(serde_json::to_string(&event.payload)?)
    .bind(&event.timestamp)
    .execute(pool)
    .await
    .map(|result| result.last_insert_rowid())
}
```

---

## 3. API Surface Mapping

### 3.1 Complete Endpoint Inventory

**REST Endpoints** (supervisor/main.py:337-898)

| Method | Path | Purpose | Request Model | Response Model |
|--------|-------|---------|---------------|----------------|
| GET | `/health` | Health check | - | `{"status": "ok", "ray": "connected"}` |
| POST | `/undo` | Undo file changes | `count: int` | `{"restored_files": [...], "count": N}` |
| POST | `/work_item` | Create/update work item | `WorkItemPayload` | `{"work_item": {...}}` |
| GET | `/work_item/{work_item_id}` | Get work item | - | `{"work_item": {...}}` |
| GET | `/work_items` | List work items | `status?, limit` | `{"work_items": [...]}` |
| POST | `/run` | Create run | `RunCreatePayload` | `{"run": {...}}` |
| PATCH | `/run/{run_id}` | Update run | `RunUpdatePayload` | `{"run": {...}}` |
| GET | `/run/{run_id}` | Get run with inputs | - | `{"run": {...}, "inputs": [...]}` |
| GET | `/runs` | List runs | `status?, limit` | `{"runs": [...]}` |
| GET | `/runs/{run_id}` | Get run with work item | - | `{"run": {...}, "inputs": [...]}` |
| GET | `/runs/{run_id}/timeline` | Get run timeline | - | Timeline events |
| POST | `/run/{run_id}/note` | Add run note | `RunNotePayload` | `{"ok": true}` |
| POST | `/run/{run_id}/verify` | Add verification | `RunVerificationPayload` | `{"ok": true}` |
| POST | `/run/{run_id}/commit_request` | Request commit | `RunCommitRequestPayload` | `{"ok": true}` |
| GET | `/state/ahdb` | Get AHDB state | - | `{"ahdb": {...}}` |
| POST | `/projection/rebuild` | Rebuild projections | `ProjectionRebuildPayload` | `{"replayed": N}` |
| GET | `/events/stream` | SSE event stream | `since_seq, event_type` | SSE text/event-stream |
| GET | `/git/status` | Git status | - | Git status dict |
| GET | `/git/log` | Git log | `count` | `{"commits": [...]}` |
| GET | `/git/diff` | Git diff | `base, head, stat` | Diff string |
| POST | `/git/checkpoint` | Create checkpoint | `message` | `{"commit_sha": "...", ...}` |
| POST | `/git/revert` | Revert to SHA | `sha, dry_run` | Revert result |
| GET | `/git/last_good` | Get last good SHA | - | `{"last_good": "..."}` |
| POST | `/git/rollback` | Rollback | `dry_run` | Rollback result |
| POST | `/sandbox/create` | Create sandbox | `SandboxCreatePayload` | `{"sandbox_id": "...", ...}` |
| POST | `/sandbox/destroy` | Destroy sandbox | `sandbox_id` | `{"success": true}` |
| POST | `/sandbox/exec` | Execute command | `SandboxExecPayload` | `{"return_code": ..., stdout, stderr}` |
| POST | `/sandbox/process/stop` | Stop process | `SandboxProcessStopPayload` | `{"success": true}` |
| POST | `/sandbox/proxy` | Open proxy | `SandboxProxyPayload` | `{"url": "...", port}` |
| POST | `/sandbox/checkpoint` | Checkpoint sandbox | `SandboxCheckpointPayload` | Checkpoint dict |
| POST | `/sandbox/restore` | Restore checkpoint | `SandboxRestorePayload` | `{"success": true}` |
| GET | `/frontend/url` | Get frontend URL | - | `{"url": "..."}` |
| GET | `/observability/context-heatmap` | Context heatmap | `since_seq, until_seq, limit` | Heatmap nodes/edges |
| GET | `/observability/ray` | Ray status | - | Ray cluster info |
| GET | `/observability/file` | Read project file | `path` | File content |
| POST | `/agent/audit` | Run auditor | `AuditRequest` | SSE stream |
| GET | `/agent/audits` | List audits | `limit` | Audit critiques |

**WebSocket Endpoint** (supervisor/main.py:885-1029)

| Path | Purpose | Message Types |
|------|---------|--------------|
| `/agent` | Agent execution streaming | See 3.2 below |

**SSE Endpoint** (supervisor/main.py:573-639)

| Path | Purpose | Event Format |
|------|---------|--------------|
| `/events/stream` | Real-time event streaming | `data: {json}\n\n` |

### 3.2 WebSocket Protocol Specification

**Connection Flow:**

```typescript
// Frontend connects
const ws = new WebSocket('ws://localhost:8001/agent?session=...');

// Supervisor extracts session
const token = extract_session_token(headers);
const session = get_auth_store().verify_session(token);

// Session required if CHOIR_AUTH_REQUIRED=1
if (AUTH_REQUIRED && !session) {
    ws.close(4401);  // Unauthorized
    return;
}
```

**Message Types (Frontend → Supervisor):**

```typescript
interface PromptMessage {
    prompt: string;
    run_id?: string;      // For followup
    input_kind?: "initial" | "followup";
}

// Frontend sends:
ws.send(JSON.stringify({
    prompt: "do this task",
    run_id: existing_run_id || undefined,
    input_kind: existing_run_id ? "followup" : "initial"
}));
```

**Message Types (Supervisor → Frontend):**

```typescript
interface EnqueuedMessage {
    type: "enqueued";
    content: {
        work_item_id: string;
        run_id: string;
        status: "queued";
    };
}

interface ThinkingMessage {
    type: "thinking";
    content: string;  // Progressive deltas
}

interface ToolUseMessage {
    type: "tool_use";
    content: {
        tool: string;      // tool_name
        input: string;     // JSON of tool arguments
    };
}

interface ToolResultMessage {
    type: "tool_result";
    content: {
        tool: string;
        result: any;
    };
}

interface TextMessage {
    type: "text";
    content: string;  // Final assistant response
}

interface VerificationMessage {
    type: "verification";
    content: {
        run: {...};
        verifier_plan: {...};
        results: [{id: string, status: string}];
    };
}

interface ErrorMessage {
    type: "error";
    content: string;
}
```

**Rate Limiting** (supervisor/main.py:976-996):

```python
# Config
MAX_PROMPT_CHARS = 20000
WS_RATE_WINDOW_SECONDS = 10
WS_MAX_PROMPTS_PER_WINDOW = 5

# Implementation
recent_prompts = deque(maxlen=100)
while True:
    prompt = await websocket.receive_json()
    if len(prompt) > MAX_PROMPT_CHARS:
        send_error("Prompt too large")
        continue

    now = time.monotonic()
    while recent_prompts and now - recent_prompts[0] > WS_RATE_WINDOW_SECONDS:
        recent_prompts.popleft()

    if len(recent_prompts) >= WS_MAX_PROMPTS_PER_WINDOW:
        send_error("Rate limit exceeded")
        continue

    recent_prompts.append(now)
    await process_prompt(prompt)
```

### 3.3 SSE Event Contract

**Format:**

```text
data: {"event_type":"...","payload":{...},"seq":123,"timestamp":"2026-01-30T12:34:56Z","event_id":"uuid-xxx"}

data: {"event_type":"...","payload":{...},"seq":124,...}

: heartbeat  (keepalive every 30s)
```

**Event Structure:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub seq: i64,
    pub timestamp: String,  // ISO 8601
    pub event_id: String,
}
```

**Backend-to-Bridge Pattern** (for BAML/Sandbox from Rust):

```
┌─────────────────────────────────────────────────────────────┐
│              Rust Supervisor (Actix)                    │
│  - HTTP/WS/SSE endpoints                                │
│  - SQLite direct access                                     │
│  - Ray-like actor model (tokio channels)                  │
└───────────────────┬─────────────────────────────────────┘
                    │ HTTP / gRPC
                    ▼
┌─────────────────────────────────────────────────────────────┐
│        Python Bridge Server (FastAPI)                   │
│  - BAML client (Anthropic/Bedrock)                      │
│  - Sprites.dev sandbox adapter                               │
│  - Local subprocess execution                                   │
│  - Simple HTTP API for Rust to call                        │
└─────────────────────────────────────────────────────────────┘
```

**Rust → Python Bridge API:**

```rust
// Rust calls Python for BAML inference
#[derive(Serialize)]
pub struct PlanActionRequest {
    pub messages: Vec<Message>,
    pub system_context: String,
    pub available_tools: String,
}

pub async fn call_baml_python(request: &PlanActionRequest) -> Result<AgentPlan, reqwest::Error> {
    let client = reqwest::Client::new();
    client.post("http://localhost:9001/baml/plan_action")
        .json(request)
        .await?
        .json()
        .await
}

// Rust calls Python for sandbox operations
pub async fn sandbox_create_python(config: &SandboxConfig) -> Result<SandboxHandle, reqwest::Error> {
    let client = reqwest::Client::new();
    client.post("http://localhost:9001/sandbox/create")
        .json(config)
        .await?
        .json()
        .await
}
```

**Python → Rust Event Publishing:**

```python
# Python bridge publishes events back to Rust via HTTP POST
import httpx

async def publish_to_rust(event: ChoirEvent):
    async with httpx.AsyncClient() as client:
        await client.post(
            "http://localhost:8001/internal/events",
            json=event.to_dict(),
            headers={"X-Internal-Auth": "secret"}
        )
```

---

## 5. Database Schema & Migration

### 5.1 Complete Schema Inventory

**Tables** (supervisor/db.py:142-316):

```sql
-- Core event log (append-only source of truth)
CREATE TABLE IF NOT EXISTS events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    nats_seq INTEGER,  -- OBSOLETE after NATS removal
    event_id TEXT,
    timestamp TEXT NOT NULL,
    type TEXT NOT NULL,
    payload JSON NOT NULL
);

CREATE INDEX idx_events_type ON events(type);
CREATE INDEX idx_events_timestamp ON events(timestamp);
CREATE INDEX idx_events_nats_seq ON events(nats_seq);  -- DROP after migration

-- Materialized: file state
CREATE TABLE IF NOT EXISTS files (
    path TEXT PRIMARY KEY,
    content_hash TEXT,
    blob_url TEXT,
    updated_at TEXT NOT NULL
);

-- Materialized: conversations
CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    title TEXT,
    last_seq INTEGER
);

-- Materialized: messages
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER REFERENCES conversations(id),
    event_seq INTEGER REFERENCES events(seq),
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL
);

CREATE INDEX idx_messages_conversation ON messages(conversation_id);

-- Materialized: tool calls
CREATE TABLE IF NOT EXISTS tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_seq INTEGER REFERENCES events(seq),
    tool_call_id TEXT,
    conversation_id INTEGER REFERENCES conversations(id),
    tool_name TEXT NOT NULL,
    tool_input JSON NOT NULL,
    tool_result JSON,
    timestamp TEXT NOT NULL
);

-- Materialized: AHDB state vector
CREATE TABLE IF NOT EXISTS ahdb_state (
    key TEXT PRIMARY KEY,
    value JSON NOT NULL,
    updated_at TEXT NOT NULL
);

-- Materialized: AHDB deltas
CREATE TABLE IF NOT EXISTS ahdb_deltas (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_seq INTEGER REFERENCES events(seq),
    delta JSON NOT NULL,
    timestamp TEXT NOT NULL
);

-- Proposed AHDB deltas (not asserted)
CREATE TABLE IF NOT EXISTS ahdb_proposals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_seq INTEGER REFERENCES events(seq),
    run_id TEXT,
    delta JSON NOT NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- Work items (persisted work queue)
CREATE TABLE IF NOT EXISTS work_items (
    id TEXT PRIMARY KEY,
    description TEXT NOT NULL,
    acceptance_criteria TEXT,
    required_verifiers JSON,
    risk_tier TEXT,
    dependencies JSON,
    status TEXT NOT NULL,
    parent_id TEXT,
    runner_id TEXT,
    run_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Runs (one work item per run)
CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    work_item_id TEXT REFERENCES work_items(id),
    status TEXT NOT NULL,
    mode TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT
);

-- Run notes (typed)
CREATE TABLE IF NOT EXISTS run_notes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT REFERENCES runs(id),
    note_type TEXT NOT NULL,
    body JSON NOT NULL,
    event_seq INTEGER REFERENCES events(seq),
    created_at TEXT NOT NULL
);

-- Verifier attestations (recorded per run)
CREATE TABLE IF NOT EXISTS run_verifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT REFERENCES runs(id),
    attestation JSON NOT NULL,
    event_seq INTEGER REFERENCES events(seq),
    created_at TEXT NOT NULL
);

-- Commit requests (director approval gate)
CREATE TABLE IF NOT EXISTS run_commit_requests (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT REFERENCES runs(id),
    payload JSON NOT NULL,
    event_seq INTEGER REFERENCES events(seq),
    created_at TEXT NOT NULL
);

-- Git checkpoints
CREATE TABLE IF NOT EXISTS checkpoints (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    commit_sha TEXT NOT NULL,
    last_event_seq INTEGER NOT NULL,
    last_nats_seq INTEGER,  -- DROP after migration
    created_at TEXT NOT NULL,
    message TEXT
);

CREATE TABLE IF NOT EXISTS projection_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sync_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Run inputs (initial prompts + follow-ups)
CREATE TABLE IF NOT EXISTS run_inputs (
    id TEXT PRIMARY KEY,
    run_id TEXT REFERENCES runs(id),
    prompt TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- User settings (provider config, etc)
CREATE TABLE IF NOT EXISTS user_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### 5.2 Migration Strategy: Direct SQLite Handoff

**Recommended Approach: Dual-Write Cutover**

**Phase 1: Preparation (1 week)**

1. Add migration tracking table to Python:
```sql
CREATE TABLE migration_lock (
    writer TEXT PRIMARY KEY,  -- "python" or "rust"
    acquired_at TEXT NOT NULL,
    heartbeat TEXT
);
```

2. Python supervisor adds heartbeat:
```python
while True:
    db.execute("""
        UPDATE migration_lock
        SET heartbeat = datetime('now')
        WHERE writer = 'python'
    """)
    await asyncio.sleep(5)
```

3. Rust supervisor reads lock before writes:
```rust
async fn acquire_write_lock(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    let lock = sqlx::query!(
        r#"SELECT writer FROM migration_lock WHERE writer = 'python'"#
    ).fetch_one(pool).await?;

    // If python holds lock, rust is read-only
    Ok(lock.is_none())
}
```

**Phase 2: Read-Only Shadow (2 weeks)**

1. Deploy Rust supervisor on alternate port (8002)
2. Rust reads existing SQLite directly
3. Rust applies same projection logic (exact parity)
4. Compare event counts, projection state between Python/Rust
5. No writes from Rust yet (read-only mode)

**Phase 3: Gradual Cutover (1 week)**

1. Update frontend to support both ports with fallback:
```typescript
const BACKEND_PORTS = [8001, 8002];  // Python, Rust
let current_port = 0;

for (const port of BACKEND_PORTS) {
    try {
        await testConnection(port);
        current_port = port;
        break;
    } catch {
        continue;
    }
}
```

2. Route subset of traffic to Rust:
   - New work items → Rust
   - Existing runs → Python
3. Monitor for discrepancies
4. If issues, rollback frontend to Python-only

**Phase 4: Full Cutover (1 hour downtime)**

1. Stop Python supervisor (graceful shutdown)
2. Wait for in-flight requests (10s)
3. Rust acquires migration lock:
```rust
sqlx::query!(
    r#"INSERT OR REPLACE INTO migration_lock (writer, acquired_at)
    VALUES ('rust', datetime('now'))"#
).execute(pool).await?;
```

4. Rust starts accepting writes
5. Verify:
   - Event streaming works (SSE active)
   - WebSocket accepts connections
   - Projections update correctly
   - Ray-like bus (tokio) works
6. If verification fails:
   - Release lock, rollback to Python

**Phase 5: Cleanup (1 week)**

1. NATS-specific cleanup:
```sql
-- Drop NATS-related columns
ALTER TABLE events DROP COLUMN nats_seq;
DROP INDEX IF EXISTS idx_events_nats_seq;

-- Update event_dedupe schema (RuntimeStore)
ALTER TABLE event_dedupe DROP COLUMN nats_seq;
```

2. Update projection_state:
```sql
-- Mark migration complete
INSERT OR REPLACE INTO projection_state (key, value)
VALUES ('migration_status', 'rust_complete');
```

**Validation Checkpoints:**

- After Phase 2 (Shadow):
  ```sql
  SELECT COUNT(*) FROM events;  -- Match between Python/Rust?
  SELECT COUNT(*) FROM files;  -- Projection parity?
  ```

- After Phase 4 (Cutover):
  ```bash
  # Test SSE endpoint
  curl -N http://localhost:8001/events/stream

  # Test WebSocket
  wscat -c ws://localhost:8001/agent

  # Test work item creation
  curl -X POST http://localhost:8001/work_item \
    -H "Content-Type: application/json" \
    -d '{"description": "test"}'
  ```

**Rollback Plan:**

If Rust fails:
```sql
-- Release lock
DELETE FROM migration_lock WHERE writer = 'rust';

-- Frontend fallback to Python
# Revert frontend config or DNS
```

**Estimated Downtime**: 1 hour for final cutover, 2 weeks of dual-read mode

---

## 6. Actor System: Ray → Rust

### 6.1 Current Ray Usage

**Ray EventBus** (supervisor/ray_bus.py:26-123)

```python
@ray.remote
class EventBus:
    def __init__(self):
        self._subscribers = {}
        self._next_id = 1

    async def subscribe(self, event_type: str) -> str:
        sub_id = str(self._next_id)
        self._subscribers[sub_id] = (event_type, asyncio.Queue())
        return sub_id

    async def publish(self, event: dict):
        for event_type, queue in self._subscribers.values():
            if event_type == "*" or event_type == bus_event.event_type:
                await queue.put(bus_event)
```

**Ray Consumers:**

1. **Machine** (supervisor/machine.py:131-143)
   - Subscribes to "run.input"
   - Listens for mode execution directives

2. **AuditorWorker** (supervisor/auditor_worker.py:34-43)
   - Subscribes to "file.write"
   - Runs unilateral auditor

3. **ProjectorWorker** (supervisor/projector_worker.py:32-39)
   - Subscribes to "*"
   - Applies events to SQLite projections

### 6.2 Rust Equivalent: Tokio Channels

**Simple EventBus (MVP approach):**

```rust
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct EventBus {
    subscribers: Arc<tokio::sync::Mutex<HashMap<String, Vec<mpsc::UnboundedSender<Event>>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub async fn subscribe(&self, event_type: String) -> mpsc::UnboundedReceiver<Event> {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut subscribers = self.subscribers.lock().await;
        subscribers.entry(event_type).or_insert_with(Vec::new).push(tx);
        drop(subscribers);

        rx
    }

    pub async fn publish(&self, event_type: String, event: Event) {
        let subscribers = self.subscribers.lock().await;
        if let Some(senders) = subscribers.get("*") {
            for sender in senders {
                let _ = sender.send(event.clone());
            }
        }
        if let Some(senders) = subscribers.get(&event_type) {
            for sender in senders {
                let _ = sender.send(event.clone());
            }
        }
    }
}
```

**Structured Actor Approach (Ractor for production):**

```rust
use ractor::{Actor, ActorRef, ActorProcessingErr};

pub struct EventBusActor;

#[async_trait]
impl Actor for EventBusActor {
    type Msg = BusMessage;
    type State = HashMap<String, Vec<ActorRef<Self>>>;

    async fn pre_start(&self, myself: ActorRef<Self>) -> Result<(), ActorProcessingErr> {
        info!(target: "EventBus", "Starting event bus");
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self>,
        message: BusMessage,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            BusMessage::Subscribe { event_type, subscriber } => {
                state.entry(event_type).or_insert_with(Vec::new).push(subscriber);
            }
            BusMessage::Publish { event_type, event } => {
                if let Some(subscribers) = state.get("*") {
                    for subscriber in subscribers {
                        let _ = subscriber.send(event.clone());
                    }
                }
                if let Some(subscribers) = state.get(&event_type) {
                    for subscriber in subscribers {
                        let _ = subscriber.send(event.clone());
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
pub enum BusMessage {
    Subscribe { event_type: String, subscriber: ActorRef<EventBusActor> },
    Publish { event_type: String, event: Event },
}
```

**Message Types:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub seq: i64,
    pub user_id: String,
    pub source: String,  // "user", "agent", "system"
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: i64,  // Unix ms
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusEvent {
    pub event_id: String,
    pub seq: i64,
    pub user_id: String,
    pub source: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: i64,
    pub subject: String,  // Formatted from above
}
```

**Integration with ProjectionStore:**

```rust
impl ProjectorWorker {
    pub async fn start(&self) {
        let bus = EventBus::new();
        let mut rx = bus.subscribe("*".to_string()).await;

        loop {
            match rx.recv().await {
                Some(event) => {
                    if self.store.has_event(&event.event_id) {
                        continue;  // Dedupe
                    }
                    self.store.apply_event(event).await;
                }
                None => break,
            }
        }
    }
}
```

---

## 7. Test Coverage Requirements

### 7.1 Test File Inventory

**24 Test Files** in supervisor/tests/

| Category | Files | Purpose |
|----------|--------|---------|
| **Ray Actor Tests** | `test_machine.py`, `test_run_orchestrator.py` | Mode transitions, run orchestration |
| **Event Sourcing Tests** | `test_event_dedupe.py`, `test_replay_mode.py`, `test_ahdb_projection.py` | Event deduping, replay, projections |
| **Database Tests** | `test_db_migrations.py` | Schema migrations, projection queries |
| **Sandbox Tests** | `test_sandbox_runner.py`, `test_sandbox_provider.py`, `test_sprites_adapter.py`, `test_sprites_live.py` | Sandbox lifecycle, Sprites.dev integration |
| **Verifier Tests** | `test_verifier_runner.py`, `test_verifier_plan.py` | Verification execution, plan selection |
| **Git Tests** | `test_supervisor_git_endpoints.py` | Git operations integration |
| **Agent Tests** | `test_tools.py`, `test_chat_actor_ledger_phases.py` | Tool execution, agent phases |
| **Integration Tests** | `test_supervisor_sandbox_endpoints.py`, `test_research_experiments.py`, `test_research_runner.py` | End-to-end flows |
| **Context Tests** | `test_context_heatmap.py` | Context heatmap generation |
| **Observability Tests** | `test_doc_alignment.py` | Document parsing |
| **Deprecated** | `test_nats_integration.py` | **DELETE** (NATS being removed) |

### 7.2 Critical Test Categories

**Must-Pass Tests** (Production Requirements):

1. **Event Deduplication** (test_event_dedupe.py)
   - Events with same event_id process only once
   - Delivery count tracking
   - Status transitions: received → processing → done/failed

2. **Mode Transitions** (test_machine.py)
   - Mode engine selects correct mode based on inputs
   - Mode transitions (CALM → CURIOUS → SKEPTICAL)
   - AHDB state updates correctly

3. **Run Orchestration** (test_run_orchestrator.py)
   - Execute → Verify → SKEPTICAL flow
   - Success path: all verifiers pass, checkpoint created
   - Failure path: verifiers fail, rollback triggers

4. **Projection Rebuild** (test_replay_mode.py)
   - Rebuild from event log produces identical state
   - Event replay order preserved
   - No lost projections

5. **Sandbox Lifecycle** (test_sandbox_runner.py)
   - Create → Run → Checkpoint → Restore → Destroy
   - Process management (start/stop)
   - Proxy opening

6. **Event Contract** (test_event_contract.py)
   - All 87 event types valid
   - Normalization (uppercase → dot-delimited)
   - Legacy event type mapping

### 7.3 Rust Test Stack

**Testing Crates:**

```toml
[dev-dependencies]
actix-rt = "2.10"           # Actix test runtime
tokio-test = "0.4"            # Tokio test utilities
sqlx = { version = "0.8", features = ["testing"] }
wiremock = "0.6"              # HTTP mocking
```

**Test Patterns:**

```rust
use actix_web::{test, web, App};
use sqlx::SqlitePool;

#[actix_web::test]
async fn test_work_item_create() {
    // Setup test database
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();

    // Create test app
    let app = test::init_service(
        App::new()
            .service(create_work_item_route)
    ).await;

    // Send request
    let req = test::TestRequest::post()
        .uri("/work_item")
        .set_json(&WorkItemPayload {
            description: "test".to_string(),
            ..Default::default()
        });

    let resp = test::call_service(&app, req).await;

    // Assertions
    assert_eq!(resp.status(), 200);

    let work_item: WorkItem = test::read_body_json(resp).await;
    assert_eq!(work_item.description, "test");
}

#[actix_web::test]
async fn test_event_streaming() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let app = test::init_service(App::new().service(event_stream_route)).await;

    // Subscribe to SSE
    let req = test::TestRequest::get()
        .uri("/events/stream?since_seq=0")
        .insert_header(("Accept", "text/event-stream"));

    let mut resp = test::call_service(&app, req).await;

    // Parse SSE stream
    let bytes = test::read_body(resp).await;
    let events = parse_sse_stream(&bytes);

    assert!(events.len() > 0);
}
```

**Mocking Actors:**

```rust
use tokio::sync::mpsc;

struct FakeEventBus {
    sender: mpsc::UnboundedSender<Event>,
}

impl FakeEventBus {
    pub fn new() -> Self {
        let (tx, _rx) = mpsc::unbounded_channel();
        Self { sender: tx }
    }

    pub fn get_publisher(&self) -> EventPublisher {
        EventPublisher::new(self.sender.clone())
    }
}

struct EventPublisher {
    sender: mpsc::UnboundedSender<Event>,
    events: Arc<tokio::sync::Mutex<Vec<Event>>>,
}

impl EventPublisher {
    pub async fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
        self.events.lock().await.push(event);
    }
}
```

**Integration Test Pattern:**

```rust
#[actix_web::test]
async fn test_full_run_orchestration() {
    // Setup
    let pool = create_test_pool().await;
    let fake_sandbox = FakeSandboxRunner::new();
    let fake_bus = FakeEventBus::new();
    let store = ProjectionStore::new(pool).await;

    let orchestrator = RunOrchestrator::new(store, fake_bus.publisher(), fake_sandbox);

    // Execute
    let work_item = store.create_work_item("test").await;
    let result = orchestrator.run(&work_item.id, &execute_mock, &specs).await;

    // Assertions
    assert_eq!(result.run.status, "verified");
    assert!(fake_sandbox.checkpoints_created() > 0);
    assert!(pool.get_run(&work_item.run_id).await.is_some());
}
```

### 7.4 Coverage Targets

| Area | Python Coverage | Rust Target |
|-------|----------------|--------------|
| Event Sourcing | apply_event, replay, dedupe | sqlx transaction tests |
| Actor Orchestration | Machine, RunOrchestrator | tokio channel tests |
| Database | ProjectionStore queries | sqlx query validation |
| WebSocket | Connection, rate limiting | actix-web WebSocket tests |
| SSE | Event streaming, filtering | SSE parser tests |
| Sandbox | Create, run, checkpoint, destroy | Mock runner tests |
| Verification | VerifierRunner, VerifierPlan | Executor tests |
| Git | Checkpoint, revert, diff | Git subprocess mocking |

---

## 8. Key Challenges & Mitigations

### 8.1 Python Interop

**Challenge**: BAML is Python-only, generated clients are Python

**Options:**

1. **HTTP Bridge** (Recommended for MVP)
   - Pros: Simple, decoupled, language boundary clear
   - Cons: Extra latency, HTTP overhead

   ```rust
   #[derive(Serialize)]
   pub struct BamlPlanRequest {
       pub messages: Vec<Message>,
       pub system_context: String,
       pub available_tools: String,
   }

   pub async fn call_baml_python(req: &BamlPlanRequest) -> Result<AgentPlan, reqwest::Error> {
       reqwest::Client::new()
           .post("http://localhost:9001/baml/plan_action")
           .json(req)
           .await?
           .json()
           .await
   }
   ```

2. **pyo3 Bindings** (Better performance, complex setup)
   - Pros: Direct Python calls, no HTTP
   - Cons: Build complexity, Python runtime in Rust process

   ```rust
   use pyo3::prelude::*;

   #[pymodule]
   fn choir_baml_bridge(_py: Python, m: &PyModule) -> PyResult<()> {
       // Expose Rust structs to Python
   }

   // In Rust, call Python:
   let gil = Python::acquire_gil();
   let baml_module = gil.import("baml")?;
   let result = baml_module.call_method("PlanAction", args, kwargs)?;
   ```

3. **Port BAML to Rust** (Long-term, major effort)
   - Pros: Pure Rust, no Python
   - Cons: Complete rewrite of LLM client, BAML language

**Recommendation**: Start with HTTP bridge, evaluate performance, consider pyo3 if latency unacceptable.

### 8.2 Ray Complexity

**Challenge**: Ray provides distributed actors, object refs, automatic serialization

**Mitigation**: Start with in-process tokio channels, evaluate if distributed actors needed

**Decision Matrix:**

| Need | Ray | Tokio Channels | Ractor |
|------|------|-----------------|---------|
| Simple pub/sub (in-process) | ✅ | ✅ | ✅ |
| Distributed nodes | ✅ | ❌ | ✅ |
| Named actor references | ✅ | ❌ | ✅ |
| Automatic serialization | ✅ | ❌ | ❌ |

**Recommendation**: Use tokio channels for MVP (Ray replacement is single-node anyway). Consider Ractor if multi-node needed later.

### 8.3 SQLite Migrations

**Challenge**: Python uses SQLite with dynamic schema, migrations via _ensure_column()

**Mitigation**: Use sqlx-cli for versioned migrations

```bash
# Generate initial migration
sqlx migrate add create_events

# Migration file: migrations/20260130000000_create_events.sql
-- Up
CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY AUTOINCREMENT, ...);
CREATE INDEX idx_events_type ON events(type);

-- Down
DROP TABLE events;

# Run migrations
sqlx migrate run
```

**Rust Schema Mapping:**

```rust
use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct EventRow {
    pub seq: i64,
    pub nats_seq: Option<i64>,
    pub event_id: String,
    pub timestamp: String,
    pub r#type: String,  // Escape keyword
    pub payload: String,
}

#[derive(Debug, FromRow)]
pub struct FileRow {
    pub path: String,
    pub content_hash: String,
    pub blob_url: Option<String>,
    pub updated_at: String,
}
```

### 8.4 WebSocket Rate Limiting

**Challenge**: Python uses deque for sliding window, needs exact parity

**Mitigation**: Implement same algorithm in Rust

```rust
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    window: Duration,
    max_prompts: usize,
    recent: VecDeque<Instant>,
}

impl RateLimiter {
    pub fn new(window_secs: u64, max_prompts: usize) -> Self {
        Self {
            window: Duration::from_secs(window_secs),
            max_prompts,
            recent: VecDeque::with_capacity(max_prompts),
        }
    }

    pub fn check(&mut self) -> bool {
        let now = Instant::now();
        while let Some(&first) = self.recent.front() {
            if now.duration_since(first) > self.window {
                self.recent.pop_front();
            } else {
                break;
            }
        }

        if self.recent.len() >= self.max_prompts {
            false
        } else {
            self.recent.push_back(now);
            true
        }
    }
}
```

### 8.5 Event Replay Complexity

**Challenge**: Python replays events in order, materializes projections

**Mitigation**: Exact algorithm translation to Rust

```python
# Python (db.py:592-623)
def rebuild_projection_from_events(self) -> int:
    cursor = self.conn.execute("SELECT seq, type, payload, timestamp FROM events ORDER BY seq")
    count = 0
    for row in cursor.fetchall():
        payload = json.loads(row["payload"])
        self._materialize_projection(row["type"], payload, row["timestamp"], row["seq"])
        count += 1
    return count
```

```rust
impl ProjectionStore {
    pub async fn rebuild_from_events(&self) -> Result<i64, sqlx::Error> {
        let mut conn = self.pool.begin().await?;

        let rows = sqlx::query_as::<(i64, String, String, String)>(
            "SELECT seq, type, payload, timestamp FROM events ORDER BY seq"
        )
        .fetch_all(&mut *conn)
        .await?;

        let mut count = 0;
        for (seq, event_type, payload, timestamp) in rows {
            let payload: serde_json::Value = serde_json::from_str(&payload)?;
            self.materialize_projection(&mut conn, &event_type, &payload, &timestamp, seq).await?;
            count += 1;
        }

        conn.commit().await?;
        Ok(count)
    }
}
```

---

## 9. Phased Implementation Plan

### 9.1 Phase 0: Foundations (Weeks 1-2)

**Goal**: Set up Rust project structure and tooling

**Tasks:**

1. **Project Setup**
   ```bash
   cd supervisor
   cargo new --name choir-supervisor --lib
   cd choir-supervisor
   ```

2. **Cargo.toml Dependencies**
   ```toml
   [package]
   name = "choir-supervisor"
   version = "0.1.0"
   edition = "2021"

   [dependencies]
   actix-web = "4.9"
   actix-cors = "0.7"
   tokio = { version = "1.40", features = ["full"] }
   sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "chrono"] }
   serde = { version = "1.0", features = ["derive"] }
   serde_json = "1.0"
   chrono = { version = "0.4", features = ["serde"] }
   uuid = { version = "1.11", features = ["v4", "serde"] }
   tracing = "0.1"
   tracing-subscriber = { version = "0.3", features = ["env-filter"] }

   [dev-dependencies]
   actix-rt = "2.10"
   tokio-test = "0.4"
   sqlx = { version = "0.8", features = ["testing"] }
   ```

3. **Directory Structure**
   ```
   supervisor-rust/
   ├── Cargo.toml
   ├── src/
   │   ├── main.rs
   │   ├── db.rs
   │   ├── models.rs
   │   ├── handlers/
   │   │   ├── mod.rs
   │   │   ├── work_items.rs
   │   │   ├── runs.rs
   │   │   ├── events.rs
   │   │   ├── git.rs
   │   │   └── sandbox.rs
   │   ├── actors/
   │   │   ├── mod.rs
   │   │   ├── event_bus.rs
   │   │   ├── machine.rs
   │   │   └── orchestrator.rs
   │   └── lib.rs
   ├── migrations/
   └── tests/
       ├── integration.rs
       └── e2e.rs
   ```

4. **Initial Build**
   ```bash
   cargo check
   cargo test
   ```

**Success Criteria**:
- ✅ Cargo project builds
- ✅ Test suite executes (even if empty)
- ✅ Clippy passes
- ✅ Database migrations run

### 9.2 Phase 1: Event Store + SSE (Weeks 3-4)

**Goal**: Event sourcing with SQLite and SSE streaming

**Tasks:**

1. **Database Models** (src/db.rs)
   ```rust
   use sqlx::{SqlitePool, FromRow};
   use chrono::{DateTime, Utc};
   use serde::{Serialize, Deserialize};

   #[derive(Debug, FromRow, Serialize, Deserialize)]
   pub struct Event {
       pub seq: i64,
       pub nats_seq: Option<i64>,  // Keep for now, drop later
       pub event_id: String,
       pub timestamp: DateTime<Utc>,
       pub r#type: String,
       pub payload: serde_json::Value,
   }

   #[derive(Debug, FromRow, Serialize, Deserialize)]
   pub struct FileState {
       pub path: String,
       pub content_hash: String,
       pub blob_url: Option<String>,
       pub updated_at: DateTime<Utc>,
   }
   // ... (all projection structs)
   ```

2. **ProjectionStore Implementation**
   - SQLite connection pool
   - `apply_event()` method
   - `_materialize_projection()` (match Python logic exactly)
   - `rebuild_projection_from_events()`
   - Projection queries (get_events, get_run, etc.)

3. **SSE Handler** (src/handlers/events.rs)
   ```rust
   use actix_web::{web, HttpResponse, Responder};
   use tokio::sync::mpsc;

   pub async fn event_stream(
       query: web::Query<StreamQuery>,
       pool: web::Data<SqlitePool>,
   ) -> impl Responder {
       let since_seq = query.since_seq.unwrap_or(0);

       // Get historical events
       let events = db::get_events(&pool, since_seq, &query.event_type).await;

       // Subscribe to live events
       let (tx, mut rx) = mpsc::unbounded_channel();
       let bus = EventBus::new();
       let sub_id = bus.subscribe(query.event_type.clone().unwrap_or("*".to_string())).await;

       tokio::spawn(async move {
           while let Some(event) = rx.recv().await {
               let line = format!(
                   "data: {}\n\n",
                   serde_json::to_string(&event).unwrap()
               );
               // Stream to client
               // (Use streaming response builder)
           }
       });

       // Response body with SSE stream
       HttpResponse::Ok()
           .content_type("text/event-stream")
           .body(...)  // Stream body
   }
   ```

4. **Tests**
   - `test_event_insertion.rs`: Event deduping
   - `test_projection_rebuild.rs`: Replay correctness
   - `test_sse_stream.rs`: SSE format validation

**Success Criteria**:
- ✅ Events insert into SQLite correctly
- ✅ SSE endpoint returns valid format
- ✅ Historical events replayed
- ✅ Live events streamed
- ✅ Test coverage > 80% for event paths

### 9.3 Phase 2: REST Endpoints (Weeks 5-7)

**Goal**: All work item, run, git endpoints

**Tasks:**

1. **Handlers**
   - `src/handlers/work_items.rs` - work_item CRUD
   - `src/handlers/runs.rs` - run CRUD + notes + verification
   - `src/handlers/git.rs` - checkpoint, revert, status, log
   - `src/handlers/sandbox.rs` - create, exec, checkpoint, destroy

2. **Request/Response Models** (src/models.rs)
   ```rust
   #[derive(Debug, Deserialize)]
   pub struct WorkItemPayload {
       pub id: Option<String>,
       pub description: Option<String>,
       pub acceptance_criteria: Option<String>,
       pub required_verifiers: Option<Vec<String>>,
       pub risk_tier: Option<String>,
       pub dependencies: Option<Vec<String>>,
       pub status: Option<String>,
       pub parent_id: Option<String>,
   }

   #[derive(Debug, Serialize)]
   pub struct WorkItemResponse {
       pub work_item: WorkItem,
   }
   ```

3. **Main App Assembly** (src/main.rs)
   ```rust
   use actix_web::{App, HttpServer};
   use actix_cors::Cors;

   #[actix_web::main]
   async fn main() -> std::io::Result<()> {
       env_logger::init();

       let pool = SqlitePool::connect(&db_url()).await.unwrap();

       HttpServer::new(move || {
           App::new()
               .app_data(pool.clone())
               .wrap(Cors::permissive())
               .service(work_items_routes)
               .service(runs_routes)
               .service(git_routes)
               .service(sandbox_routes)
       })
       .bind(("0.0.0.0", 8001))?
       .run()
       .await
   }
   ```

4. **Tests**
   - Endpoint integration tests
   - Request/response serialization
   - Database integration

**Success Criteria**:
- ✅ All 32 REST endpoints functional
- ✅ Request/response JSON matches Python exactly
- ✅ Database operations correct
- ✅ CORS headers present
- ✅ Test coverage > 70% for REST paths

### 9.4 Phase 3: WebSocket + Agent (Weeks 8-10)

**Goal**: WebSocket protocol and agent execution flow

**Tasks:**

1. **WebSocket Handler** (src/handlers/websocket.rs)
   ```rust
   use actix_web::{web, HttpRequest, HttpResponse, Message};
   use actix::ws;

   pub async fn agent_websocket(
       req: HttpRequest,
       stream: web::Payload,
       pool: web::Data<SqlitePool>,
   ) -> Result<HttpResponse, Error> {
       // Extract session
       let session = extract_session(&req);
       if AUTH_REQUIRED && session.is_none() {
           return Ok(HttpResponse::Unauthorized().finish());
       }

       // Start WebSocket
       ws::start(
           WebSocketSession::new(session, pool),
           &req,
           stream,
       )
   }

   struct WebSocketSession {
       session_id: Option<String>,
       rate_limiter: RateLimiter,
   }

   impl ws::Handler for WebSocketSession {
       async fn on_message(&mut self, msg: Message, ctx: &mut ws::WebsocketContext) {
           match msg {
               Message::Text(text) => {
                   let prompt_msg: PromptMessage = serde_json::from_str(&text)?;
                   self.handle_prompt(prompt_msg).await?;
               }
               _ => {}
           }
       }
   }
   ```

2. **Rate Limiter** (src/rate_limiter.rs)
   - Same algorithm as Python
   - MAX_PROMPT_CHARS: 20000
   - WS_RATE_WINDOW_SECONDS: 10
   - WS_MAX_PROMPTS_PER_WINDOW: 5

3. **Machine Orchestration** (src/actors/machine.rs)
   - Mode selection logic (mode_engine.rs translation)
   - Run input handling
   - Mode transitions

4. **Python Bridge Server** (supervisor-rust/python-bridge/)
   ```python
   # Simple FastAPI server called by Rust
   from fastapi import FastAPI
   import uvicorn

   app = FastAPI()

   @app.post("/baml/plan_action")
   async def plan_action(req: dict):
       from baml import b
       result = await b.PlanAction(**req)
       return result

   if __name__ == "__main__":
       uvicorn.run(app, host="127.0.0.1", port=9001)
   ```

5. **Tests**
   - WebSocket connection tests
   - Message protocol tests
   - Rate limiting tests
   - Agent execution mock tests

**Success Criteria**:
- ✅ WebSocket accepts connections
- ✅ Message protocol matches Python exactly
- ✅ Rate limiting enforced
- ✅ Agent orchestration flow works
- ✅ Python bridge responds correctly

### 9.5 Phase 4: Ray → Tokio Actors (Weeks 11-13)

**Goal**: Replace Ray EventBus with tokio channels

**Tasks:**

1. **EventBus** (src/actors/event_bus.rs)
   - Tokio mpsc implementation
   - Subscribe/unsubscribe
   - Publish to all matching subscribers

2. **Workers**
   - ProjectorWorker (replaces Python projector_worker.py)
   - AuditorWorker (replaces Python auditor_worker.py)

3. **Actor Integration**
   - Machine subscribes to run.input
   - Projector subscribes to *
   - Auditor subscribes to file.write

4. **Tests**
   - Actor messaging tests
   - Concurrency tests
   - Event deduping tests

**Success Criteria**:
- ✅ EventBus publishes to all subscribers
- ✅ Subscribers receive events in order
- ✅ Workers process events correctly
- ✅ No message loss under load
- ✅ Tests pass for actor patterns

### 9.6 Phase 5: Full Feature Parity (Weeks 14-16)

**Goal**: All remaining features, verification, sandbox

**Tasks:**

1. **Verification**
   - VerifierRunner (call Python bridge or direct BAML)
   - VerifierPlan (plan selection logic)
   - BAML analysis integration

2. **Sandbox**
   - SandboxRunner (call Python bridge for Sprites.dev)
   - LocalSandboxRunner (direct Rust for local)
   - Checkpoint/restore

3. **Git Operations**
   - All git_ops.rs functionality
   - Subprocess execution
   - .choirignore filtering

4. **Context Heatmap**
   - Heatmap generation logic
   - Edge/node calculation

5. **Observability**
   - `/health` endpoint
   - `/observability/ray` → `/observability/actors`
   - Metrics collection

6. **Integration Tests**
   - Full run orchestration flows
   - E2E: work_item → run → verify → checkpoint
   - Sandbox lifecycle tests with real Sprites.dev

7. **Performance Tests**
   - Load test SSE streaming
   - Load test WebSocket connections
   - Database query performance

**Success Criteria**:
- ✅ All Python supervisor tests have Rust equivalents
- ✅ Integration tests pass end-to-end
- ✅ Production workloads handled
- ✅ Performance meets or exceeds Python
- ✅ Zero data loss in migration

### 9.7 Phase 6: Cutover & Cleanup (Week 17)

**Goal**: Production deployment, NATS removal, Python deprecation

**Tasks:**

1. **Migration Execution**
   - Dual-read shadow mode (2 weeks)
   - Gradual traffic split
   - Full cutover

2. **NATS Cleanup**
   - Drop nats_seq column
   - Update RuntimeStore (event_dedupe)
   - Documentation updates

3. **Python Deprecation**
   - Mark Python supervisor as deprecated
   - Update docs, run.sh scripts
   - Add migration warnings to Python logs

4. **Monitoring**
   - Grafana/Prometheus dashboards
   - Error rate tracking
   - Performance baselines

5. **Rollback Plan Verification**
   - Test rollback to Python
   - Verify data integrity
   - Document rollback procedure

**Success Criteria**:
- ✅ Rust supervisor in production
- ✅ Python supervisor stopped
- ✅ All traffic on Rust
- ✅ No user-facing downtime
- ✅ NATS dependencies removed
- ✅ Monitoring shows healthy system

---

## 10. Risk Assessment

### 10.1 Technical Risks

| Risk | Impact | Likelihood | Mitigation |
|-------|---------|------------|-------------|
| **Python Interop Latency** | Performance regression | Medium | Profile HTTP bridge, consider pyo3 if >100ms overhead |
| **Actor Model Complexity** | Development delay | High | Start with simple tokio channels, avoid over-engineering |
| **SQLite Schema Drift** | Migration failure | Low | Use sqlx migrations, strict schema validation |
| **WebSocket Protocol Mismatch** | Frontend breaks | Medium | Exact protocol preservation, WebSocket conformance tests |
| **SSE Race Conditions** | Event loss | Low | Dedupe by event_id, order by seq |
| **Event Replay Correctness** | Data corruption | Medium | Port Python logic line-by-line, extensive tests |
| **Rate Limiting Edge Cases** | DoS vulnerability | Low | Test sliding window boundaries, exact Python algorithm |
| **Sandbox Integration** | Isolation failure | Medium | Keep Python bridge for Sprites.dev, direct Rust for local |

### 10.2 Project Risks

| Risk | Impact | Likelihood | Mitigation |
|-------|---------|------------|-------------|
| **Timeline Overrun** | Delayed launch | Medium | Phase gating, MVP-first approach, defer optional features |
| **Resource Shortage** | Slower development | Low | 1-2 engineers, clear priorities |
| **Test Coverage Gaps** | Production bugs | Medium | Port all Python tests, add regression tests |
| **Knowledge Transfer** | Support burden | Low | Comprehensive documentation, pair programming on critical paths |
| **Rollback Complexity** | Extended outage | Low | Test rollback procedures, keep Python supervisor warm |

### 10.3 Risk Response Matrix

**Critical Risks (Must Mitigate):**

1. **Event Loss During Migration**
   - Mitigation: Event deduping by event_id prevents duplicates
   - Contingency: If Rust loses events, replay from SQLite

2. **Frontend Breaking Changes**
   - Mitigation: Exact WebSocket/SSE protocol preservation
   - Contingency: Canary deployment, monitor frontend errors

3. **BAML Integration Failure**
   - Mitigation: HTTP bridge with fallback
   - Contingency: Manual agent execution path (no LLM)

4. **Database Corruption**
   - Mitigation: SQLite WAL mode, transactional updates
   - Contingency: Backup before migration, verify checksums

**Acceptable Risks (Monitor):**

1. **Performance Degradation**
   - Target: Rust ≥ Python performance
   - If slower: Optimize, profile, consider alternative approaches

2. **Feature Gaps**
   - Target: 100% feature parity for critical paths
   - Monitor usage of deprecated features, deprecate if unused

---

## 11. Decision Criteria

### 11.1 Go Decision: Proceed with Rust MVP

**Trigger:**
- Ray stability issues persist (actor creation, message loss, performance)
- OR Technical lead approval for rewrite

**Success Criteria for MVP:**
- [ ] Event store with SSE streaming functional
- [ ] Work items and runs CRUD endpoints functional
- [ ] WebSocket agent communication working
- [ ] Basic integration tests pass (≥70% coverage)
- [ ] Performance matches Python (±20%)

**Timebox**: 8 weeks to complete Phases 0-3

**Decision Point**: After Phase 3, evaluate:

```
If (All MVP success criteria met AND Rust performance ≥ Python):
    Proceed to Phase 4-6 (full parity)
Else:
    Re-evaluate (continue Python vs Rust approach)
```

### 11.2 No-Go Decision: Continue Python

**Trigger:**
- Rust MVP fails significantly (≥50% of criteria)
- OR Unforeseen blocker (BAML integration impossible, timeline)

**Recovery Plan:**
- Keep Python supervisor as primary
- Defer Rust work or pivot to incremental improvements

### 11.3 Hybrid Decision: Gradual Migration

**Trigger:**
- Rust MVP succeeds but risks remain
- OR Timeline pressure (need incremental wins)

**Approach**:
- Migrate non-critical paths first
  - Event streaming (Rust) ← Python
  - Git operations (Rust) ← Python
  - Read-only queries (Rust) ← Python
- Keep critical paths in Python longer
  - Agent execution (Python)
  - Write operations (Python)
- Gradually shift traffic over 3-6 months

---

## 12. Conclusion

### 12.1 Summary of Findings

**Feasibility**: ✅ **HIGH**

The Rust Actix rewrite is technically feasible with clear migration paths:

1. **Technology Stack Mature**
   - Actix-web: Production-tested, WebSocket/SSE support
   - Tokio: Proven async runtime, channels for actors
   - SQLX: Type-safe SQLite, migration tooling
   - Serde: Reliable serialization, JSON support

2. **Architecture Clear**
   - Ray → Tokio channels (simple, in-process)
   - FastAPI → Actix-web (endpoint parity achievable)
   - SQLite → SQLX (schema mapping straightforward)
   - BAML → Python HTTP bridge (low risk, fallback to pyo3)

3. **Migration Path Safe**
   - Dual-write cutover prevents data loss
   - Exact protocol preservation protects frontend
   - Test parity ensures correctness
   - Phased rollout limits blast radius

**Key Challenges** (addressable):

1. **Python Interop** - HTTP bridge adds latency but manageable
2. **Event Replay** - Careful porting of projection logic required
3. **Timeline** - 4-6 months realistic for full parity

### 12.2 Recommendation

**Proceed with Phased Rewrite** with following sequence:

```
Week 1-2:   Phase 0 (Foundations)
Week 3-4:   Phase 1 (Event Store + SSE)
Week 5-7:   Phase 2 (REST Endpoints)
Week 8-10:  Phase 3 (WebSocket + Agent)
Week 11-13: Phase 4 (Actors)
Week 14-16: Phase 5 (Full Parity)
Week 17:      Phase 6 (Cutover)
```

**Decision Points:**

1. **After Phase 3** (Week 10): Evaluate MVP viability
2. **After Phase 4** (Week 13): Assess actor model needs
3. **After Phase 5** (Week 16): Production readiness review

**Critical Success Factors:**

- ✅ Exact protocol preservation (WebSocket, SSE, REST)
- ✅ Complete test parity (24 test files ported)
- ✅ Zero data loss (dual-write cutover)
- ✅ Performance ≥ Python
- ✅ Operational simplicity (single binary, no Ray complexity)

### 12.3 Next Steps

**Immediate Actions (This Week):**

1. [ ] Set up Rust project structure
2. [ ] Define Cargo.toml with all dependencies
3. [ ] Port event contract to Rust models
4. [ ] Implement Phase 0: DB + SSE
5. [ ] Write first integration test (event insertion + SSE)

**Research Questions for Follow-up:**

1. BAML integration latency benchmarks (HTTP vs pyo3)
2. Sprites.dev API for direct Rust calls
3. Actix WebSocket performance under load
4. SQLX migration best practices
5. Rust actor framework comparison (tokio vs ractor)

---

## Appendix A: Event Contract Reference

**All 87 Event Types** (exact strings to preserve):

```rust
pub const EVENT_TYPES: &[&str] = &[
    // Core
    "file.write", "file.delete", "file.move",
    "message", "tool.call", "tool.result",
    "window.open", "window.close",
    "checkpoint", "undo",
    "mode.start", "mode.stop", "mode.update", "mode.heartbeat",
    "run.input", "run.started", "run.finished",
    "artifact.create", "artifact.pointer",

    // Document
    "document.create", "document.edit", "document.snapshot", "document.restore",
    "ai.suggestion", "ai.suggestion.accept", "ai.suggestion.reject",

    // Notes (AHDB)
    "note.observation", "note.hypothesis", "note.hyperthesis", "note.conjecture",
    "note.status", "note.request.help", "note.request.verify",

    // Receipts
    "receipt.read", "receipt.patch", "receipt.verifier", "receipt.net", "receipt.db",
    "receipt.export", "receipt.publish", "receipt.context.footprint",
    "receipt.verifier.results", "receipt.verifier.attestations",
    "receipt.dlq", "receipt.commit", "receipt.mode.transition",
    "receipt.ahdb.delta", "receipt.evidence.set.hash", "receipt.retrieval",
    "receipt.conjecture.set", "receipt.policy.decision.tokens",
    "receipt.security.attestations", "receipt.hyperthesis.delta",
    "receipt.expansion.plan", "receipt.projection.rebuild", "receipt.attack.report",
    "receipt.disclosure.objects", "receipt.mitigation.proposals", "receipt.preference.decision",
    "receipt.timeout",

    // Settings
    "provider.changed", "provider.test"
];
```

---

## Appendix B: Rust Code Templates

**EventBus Template:**

```rust
use tokio::sync::mpsc;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Event {
    pub event_id: String,
    pub seq: i64,
    pub event_type: String,
    pub payload: serde_json::Value,
}

pub struct EventBus {
    subscribers: Arc<tokio::sync::Mutex<HashMap<String, Vec<mpsc::UnboundedSender<Event>>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub async fn publish(&self, event_type: String, event: Event) {
        let subscribers = self.subscribers.lock().await;
        if let Some(senders) = subscribers.get("*") {
            for sender in senders {
                let _ = sender.send(event.clone());
            }
        }
        if let Some(senders) = subscribers.get(&event_type) {
            for sender in senders {
                let _ = sender.send(event.clone());
            }
        }
    }

    pub async fn subscribe(&self, event_type: String) -> mpsc::UnboundedReceiver<Event> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subscribers = self.subscribers.lock().await;
        subscribers.entry(event_type).or_insert_with(Vec::new).push(tx);
        drop(subscribers);
        rx
    }
}
```

**SSE Handler Template:**

```rust
use actix_web::{web, HttpResponse, Responder};
use tokio::sync::mpsc;
use futures::stream::Stream;

pub async fn event_stream(
    query: web::Query<StreamQuery>,
    pool: web::Data<SqlitePool>,
) -> impl Responder {
    let since_seq = query.since_seq.unwrap_or(0);

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let bus = EventBus::new();
    let _sub = bus.subscribe("*".to_string()).await;

    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            let _ = tx.send(line);
        }
    });

    let stream = Stream::new(rx, move |item| {
        format!("data: {}\n\n", item)
    });

    HttpResponse::Ok()
        .content_type("text/event-stream")
        .streaming(stream, |err, _| {
            tracing::error!("SSE stream error: {:?}", err);
        })
}
```

**WebSocket Handler Template:**

```rust
use actix_web::{web, HttpRequest, HttpResponse, Message};
use actix::ws;

pub async fn agent_websocket(
    req: HttpRequest,
    stream: web::Payload,
) -> Result<HttpResponse, Error> {
    let session = extract_session(&req);
    if AUTH_REQUIRED && session.is_none() {
        return Ok(HttpResponse::Unauthorized().finish());
    }

    ws::start(WebSocketSession::new(session), &req, stream)
}

struct WebSocketSession {
    session_id: Option<String>,
}

impl ws::Handler for WebSocketSession {
    async fn on_message(&mut self, msg: Message, ctx: &mut ws::WebsocketContext) {
        match msg {
            Message::Text(text) => {
                if let Ok(prompt) = serde_json::from_str::<PromptMessage>(&text) {
                    ctx.text(serde_json::to_string(&response).unwrap());
                }
            }
            _ => {}
        }
    }
}
```

---

**End of Research Report**
