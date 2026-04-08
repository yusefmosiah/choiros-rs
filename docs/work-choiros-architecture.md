# ChoirOS Architecture — Comprehensive Analysis

> Produced for Go rewrite planning. Based on direct source code analysis of the choiros-rs repository.

---

## 1. System Overview

**ChoirOS** ("The Automatic Computer") is a multi-agent system where autonomous AI agents collaborate inside per-user isolated sandboxes. The system manages state, executes tools, and composes solutions through collective intelligence.

### What it is
A Rust monorepo containing three workspace crates (`hypervisor`, `sandbox`, `shared-types`) plus a separate WASM frontend crate (`dioxus-desktop`). The system runs on bare-metal OVH servers (x86_64-linux) with NixOS, using microVMs (cloud-hypervisor or firecracker) for per-user isolation.

### How it runs
1. **Hypervisor** (port 9090) — Axum HTTP server acting as control plane. Handles auth (WebAuthn), proxies traffic to sandboxes, manages VM lifecycle, provides a provider gateway for LLM API calls.
2. **Sandbox** (port 8080 inside VM) — Axum HTTP server inside each microVM. Hosts the actor system (ractor), event store (SQLite), and all agent logic.
3. **Frontend** — Dioxus WASM app compiled to WebAssembly, served as static assets. Renders a desktop-metaphor UI with windows, terminals, and a writer/editor.

### What it produces
- **Living documents**: Collaborative documents that agents write and revise through a structured writer actor with versioning, overlays, and patch operations.
- **Research outputs**: Multi-step research with web search, URL fetching, and citation tracking.
- **Terminal sessions**: Interactive PTY sessions that agents can use to run commands.
- **Work attestations**: Integration with `cogent` (Go binary) for work management, dispatch, and attestation.

### Deployment topology
```
OVH Bare Metal Host (NixOS)
├── Hypervisor Process (systemd)
│   ├── WebAuthn auth, session store (SQLite)
│   ├── Provider Gateway (proxies LLM API calls with secrets)
│   ├── Sandbox Registry (manages VM lifecycle)
│   └── Proxy middleware (routes /api/* to sandbox VMs)
│
├── User MicroVMs (one per user, cloud-hypervisor or firecracker)
│   ├── Sandbox Process
│   │   ├── Actor system (ractor)
│   │   ├── EventStore (SQLite, /opt/choiros/data/sandbox/)
│   │   └── HTTP API (port 8080)
│   └── data.img (virtio-blk, 2GB, persistent state)
│
├── Worker MicroVMs (shared pool, thick guest with dev tooling)
│   └── cogent serve --auto (autonomous work dispatch)
│
└── Static assets (Dioxus WASM frontend)
```

---

## 2. Crate Map

### 2.1 `hypervisor` (Control Plane)

**Purpose**: Authentication, VM lifecycle management, traffic proxying, LLM provider gateway.

**Key modules**:

| Module | Role |
|--------|------|
| `main.rs` | Axum server setup, route definitions, middleware wiring |
| `config.rs` | Environment-based config: ports, DB URL, WebAuthn RP, machine classes, provider gateway settings |
| `state.rs` | `AppState` struct: DB pool, WebAuthn, sandbox registry, provider gateway state, proxy client |
| `auth/` | WebAuthn registration/login/recovery, session management, auth middleware |
| `middleware.rs` | `require_auth` middleware, `proxy_to_sandbox` fallback that routes all non-auth traffic to the correct sandbox VM based on route pointers |
| `provider_gateway.rs` | Proxies LLM API calls from sandboxes to upstream providers (Anthropic, OpenAI, Z.AI, Kimi, Inception, OpenRouter, Tavily, Brave, Exa, AWS Bedrock). Injects real API keys. Per-sandbox rate limiting. |
| `sandbox/mod.rs` | `SandboxRegistry`: manages per-user VM lifecycle (boot, stop, hibernate, swap, branch). Memory pressure checks. Idle watchdog. |
| `sandbox/systemd.rs` | `SystemdLifecycle`: interfaces with systemd or vfkit-runtime-ctl to manage VM processes |
| `runtime_registry.rs` | Route pointer system: `main` → live role, `dev` → dev role, or → branch. Stored in SQLite. |
| `jobs.rs` | Job queue (ADR-0014 Phase 7): queued/running/completed jobs on worker VMs. Promotion API for applying results. |
| `session_store.rs` | SQLite-backed session store for tower-sessions |
| `proxy/` | Connection-pooled HTTP client for proxying to sandbox |
| `api/` | Admin endpoints: sandbox management, machine classes, jobs, promotions, heartbeat, host stats |
| `db/` | SQLite connection setup with migrations |
| `bin/` | `vfkit-runtime-ctl` binary for macOS local dev (vfkit VM management) |

**Public API (HTTP routes)**:
- Auth: `/auth/{register,login,logout,recovery,me}/...`
- Provider gateway: `/provider/v1/{provider}/{rest}` (proxied with API key injection)
- Admin: `/admin/sandboxes/...`, `/admin/jobs/...`, `/admin/machine-classes`
- Profile: `/profile/machine-class`
- Heartbeat: `/heartbeat`
- Fallback: everything else proxied to sandbox

**Dependencies**: axum, tower-http, tower-sessions, webauthn-rs, sqlx (SQLite), reqwest, dashmap, serde, tracing

### 2.2 `sandbox` (Runtime Plane)

**Purpose**: Actor-based backend hosting all agent logic, event store, and API. Runs inside each microVM.

**Key modules**:

| Module | Role |
|--------|------|
| `main.rs` | Server bootstrap: EventStore spawn, supervisor spawn, CORS, static file serving, keyless sandbox policy enforcement |
| `lib.rs` | Module declarations |
| `app_state.rs` | `AppState`: holds event store ref, supervisor ref, conductor ref. Methods to get/create actors. |
| `actors/` | The entire actor system (see §3) |
| `api/` | HTTP routes: desktop, terminal, writer, conductor, files, logs, viewer, websocket |
| `supervisor/` | Supervision tree: ApplicationSupervisor → SessionSupervisor → {Conductor,Desktop,Terminal,Researcher,Writer}Supervisor |
| `self_directed_dispatch.rs` | Integration with `cogent` CLI: loads ready work items, claims them, dispatches to agent harness |
| `baml_client/` | Auto-generated BAML client code (from baml_src definitions) |
| `markdown.rs` | Markdown rendering and processing |
| `observability/` | LLM trace emitter for structured observability of model calls |
| `tools/` | Tool implementations (bash, web_search, fetch_url, file_read/write/edit) |
| `runtime_env.rs` | TLS cert environment detection for NixOS guests |
| `paths.rs` | Sandbox-safe path resolution |

**Public API (HTTP routes)**:
- Health: `/health`
- Desktop: `/desktop/{id}`, `/desktop/{id}/windows/...`, `/desktop/{id}/apps`
- Terminal: `/api/terminals/{id}`, `/ws/terminal/{id}` (WebSocket)
- Writer: `/writer/{open,save,save-version,ensure,preview,prompt,versions,version,overlay/dismiss}`
- Conductor: `/conductor/{execute,runs,runs/{id},runs/{id}/state}`
- Files: `/files/{list,metadata,content,create,write,mkdir,rename,delete,copy}`
- Logs: `/logs/{events,latest-seq,events.jsonl,run.md}`
- Viewer: `/viewer/content`
- WebSocket: `/ws`, `/ws/logs/events`

**Dependencies**: ractor (actor framework), axum, sqlx (SQLite), baml, portable-pty, tokio, serde, chrono, ulid, reqwest, rusqlite

### 2.3 `shared-types`

**Purpose**: Shared data model between backend (sandbox) and frontend (dioxus-desktop WASM).

**Key types** (all `Serialize + Deserialize + TS`):
- `ActorId`, `Event`, `AppendEvent`, `QueryEvents` — core event system
- `DesktopState`, `WindowState`, `AppDefinition` — UI state model
- `ViewerKind`, `ViewerResource`, `ViewerDescriptor`, `ViewerRevision` — document viewing
- `WsMsg` — WebSocket message protocol (Subscribe, Send, Event, State, Error)
- `DesktopWsMessage` — Desktop WebSocket protocol (subscribe, ping/pong, window events, telemetry, writer run events)
- `DesktopTelemetryEvent`, `ConductorDocumentUpdatePayload` — live telemetry
- `ToolDef`, `ToolCall` — LLM tool definitions
- `WorkerTurnReport`, `WorkerFinding`, `WorkerLearning`, `WorkerEscalation` — structured worker output
- `ObjectiveStatus`, `PlanMode`, `FailureKind` — execution control
- `ObjectiveContract`, `ObjectiveConstraints`, `EvidenceRequirements` — delegation contracts
- `ConductorRunState`, `ConductorRunStatus`, `ConductorExecuteRequest` — conductor run model
- `ConductorAgendaItem`, `ConductorActiveCall`, `ConductorArtifact`, `ConductorDecisionEntry` — conductor internals
- `EventMetadata`, `EventLane`, `EventImportance` — event metadata for routing
- `PatchOp`, `PatchOpKind` — document patch operations (insert, delete, replace)
- `WriterRunEventBase`, `WriterRunPatchPayload`, `WriterRunChangesetPayload` — writer WebSocket events
- `CitationRecord`, `CitationRef`, `ContextItem`, `ContextSnapshot` — citation and context types

**Uses `ts-rs`** to auto-generate TypeScript types for the WASM frontend.

**Dependencies**: serde, chrono, uuid, ulid, ts-rs

### 2.4 `dioxus-desktop` (Web UI)

**Purpose**: WASM-based desktop-metaphor UI. Compiled to WebAssembly, served as static assets.

**Key modules**:
- `lib.rs` / `main.rs` — WASM entry point, launches `Desktop` component
- `api.rs` — HTTP client functions for all sandbox API endpoints (uses `gloo_net`)
- `desktop.rs` / `desktop_window.rs` — Desktop window manager: drag, resize, minimize, maximize, focus, z-ordering. Mobile-responsive.
- `terminal.rs` — xterm.js terminal integration via WebSocket
- `auth/` — Auth modal (WebAuthn login/register/recovery)
- `components/` — FilesView, LogsView, SettingsView, TraceView, WriterView
- `viewers/` — ViewerShell for rendering document content
- `interop.rs` — JS interop helpers

**Dependencies**: dioxus 0.7 (web), gloo-net, wasm-bindgen, web-sys, shared-types

---

## 3. Actor System

ChoirOS uses the **ractor** crate for its actor model. All actors communicate via typed messages with `RpcReplyPort` for request-response patterns and fire-and-forget `cast!` for async sends.

### 3.1 Supervision Tree

```
ApplicationSupervisor (one_for_one)
├── EventBusActor — pub/sub event distribution using ractor Process Groups
├── EventRelayActor — bridges EventStore polling to EventBus publishing
└── SessionSupervisor (one_for_one)
    ├── ConductorSupervisor → ConductorActor(s)
    ├── DesktopSupervisor → DesktopActor(s)
    ├── TerminalSupervisor → TerminalActor(s)
    ├── ResearcherSupervisor → ResearcherActor(s)
    └── WriterSupervisor → WriterActor(s)
```

Also standalone:
- `EventStoreActor` — spawned before the supervisor tree (foundation)

### 3.2 Actors

| Actor | Message Type | Purpose |
|-------|-------------|---------|
| **EventStoreActor** | `EventStoreMsg` | Append-only event log (SQLite). Foundation of the system. Supports: Append, GetEventsForActor, GetEventsForActorWithScope, GetRecentEvents, GetLatestSeq, GetEventBySeq, GetEventsByCorrId, GetLatestHarnessCheckpoint |
| **EventBusActor** | `EventBusMsg` | Topic-based pub/sub using ractor Process Groups. Subscribe/Publish/Unsubscribe. Wildcard topics. |
| **EventRelayActor** | `EventRelayMsg` | Polls EventStore for new events, publishes to EventBus. Bridges persistence ↔ distribution. |
| **ConductorActor** | `ConductorMsg` | Central orchestration. Receives objectives, plans execution via BAML model calls, dispatches to workers, manages run state. Messages: ExecuteTask, StartRun, GetRunState, ListRuns, CapabilityCallFinished, ProcessEvent, SubmitUserPrompt, HarnessComplete, HarnessFailed, HarnessProgress |
| **WriterActor** | `WriterMsg` | Document writing authority. Event-driven (no planning loop). Manages run documents with versioning, overlays, patch operations. Delegates to researcher/terminal via adapters. Messages: OpenDocument, SaveDocument, SubmitUserPrompt, WriterInbound, OrchestratePrompt, DelegationWorkerCompleted, etc. |
| **TerminalActor** | `TerminalMsg` | PTY process management. Spawns bash/zsh shells. I/O via WebSocket streaming. Agent-driven tool execution via the unified agent harness. |
| **ResearcherActor** | `ResearcherMsg` | Web research agent. Uses unified agent harness with BAML planning. Executes web_search, fetch_url tools. Produces findings, citations, learnings. |
| **DesktopActor** | `DesktopActorMsg` | Window state management. Owns all window state in SQLite. Supports open, close, move, resize, focus, minimize, maximize, restore. Event-sourced. |
| **MemoryActor** | `MemoryMsg` | Per-user symbolic memory. Four collections: user_inputs, version_snapshots, run_trajectories, doc_trajectories. SQLite-backed with SHA-256 dedup. |

### 3.3 The Conductor → Writer → Terminal Hierarchy

The **Conductor** is the top-level orchestrator:

1. User submits objective via `/conductor/execute`
2. Conductor calls `ConductorBootstrapAgenda` BAML function to decide which capabilities to dispatch
3. Capabilities: `writer`, `researcher`, `terminal`, `immediate_response`, `harness`
4. Each capability maps to an agenda item with a refined objective

The **Writer** is the document authority:

1. Conductor dispatches to Writer with an objective
2. Writer creates/opens a `RunDocument` (in-memory + persisted via `.qwy` format)
3. Writer can delegate subtasks to Researcher or Terminal via `WriterDelegationAdapter`
4. Delegation uses the unified `AgentHarness` loop with the `message_writer` tool for inter-actor communication
5. Writer applies patches, manages versions, overlays, and section states
6. Writer emits real-time WebSocket events (started, progress, patch, status, changeset)

The **Terminal** is the code execution engine:

1. Spawns PTY processes (bash/zsh) via `portable-pty`
2. Can run in interactive mode (user WebSocket) or agent mode (harness-driven)
3. In agent mode, uses `TerminalAdapter` implementing `WorkerPort` trait
4. Executes bash commands, file read/write/edit, web search, message_writer tool calls
5. Reports results back to Writer via `message_writer` tool or conductor via `CapabilityCallFinished`

### 3.4 The Agent Harness (Unified Execution Loop)

The `AgentHarness` provides a reusable agentic loop:

```
loop {
    1. Build context (conversation history + tool results)
    2. Call BAML `Decide` function → AgentDecision (tool_calls + message)
    3. If `finished` tool call → break with final response
    4. Execute tool calls in sequence (bash, web_search, fetch_url, file_read/write/edit, message_writer)
    5. Append tool results to conversation
    6. Check step/timeout budget
}
```

The harness is parameterized by the `WorkerPort` trait — each actor (Terminal, Researcher, Writer delegation) provides its own adapter.

There is also an **ALM (Actor Language Model) harness** (`alm.rs`, `alm_port.rs`) — a more general execution mode where the model composes its own context each turn via `ContextSource` selections and can output execution DAGs (programs with variable references, conditionals, and embedded LLM calls). The linear tool loop is a degenerate case of ALM.

### 3.5 The "AppAgent" Concept

The `appagent` concept manifests through the conductor's ability to dispatch to different "capabilities" (writer, researcher, terminal, harness). Each is an independent actor that can be scaled and supervised separately. The conductor acts as the "app-level agent" that plans and delegates.

---

## 4. MicroVM / Lease Model

### 4.1 VM Creation

VMs are created via the `SandboxRegistry` in the hypervisor:

1. **Boot flow**: `SandboxRegistry::boot_live_sandbox()` is called on hypervisor start
2. **SystemdLifecycle** or **vfkit-runtime-ctl** manages the actual VM process
3. On OVH (production): systemd templates (`cloud-hypervisor@.service` or `firecracker@.service`)
4. On macOS (dev): vfkit via `vfkit-runtime-ctl ensure --user-id <id> --runtime live --role live --port 8080`

### 4.2 VM Types

| Type | Role | Guest Profile |
|------|------|---------------|
| **User VM (live)** | Per-user sandbox, receives proxied traffic | `minimal` (2 vCPU, 1024MB) |
| **User VM (dev)** | Dev sandbox for the same user | `minimal` |
| **User VM (branch)** | Feature branch sandbox, dynamic port | `minimal` |
| **Worker VM** | Shared pool, runs `cogent serve --auto` | `worker` (thick guest with dev tooling) |

### 4.3 VM Lifecycle

```
boot → Running → (idle timeout) → Hibernated/Stopped
                → (heartbeat) → stays Running
                → (manual stop) → Stopped
```

- **Idle watchdog**: Scans every 30s, shuts down VMs idle longer than `sandbox_idle_timeout` (default 1800s)
- **Memory pressure**: Before spawning, checks `/proc/meminfo` for ≥1GB available
- **Heartbeat**: `/heartbeat` POST keeps sandbox alive without proxying

### 4.4 Authority Model

1. **Hypervisor holds all secrets**: LLM API keys are never in the sandbox. The sandbox sends LLM requests to the hypervisor's provider gateway, which injects the real API key.
2. **Keyless sandbox policy**: Sandbox on boot asserts that no forbidden provider key env vars are set. Enforced automatically in managed mode.
3. **Gateway token**: Shared token injected via kernel cmdline (`choir.gateway_token=<TOKEN>`). Extracted by systemd oneshot before sandbox starts.
4. **Non-root sandbox**: Sandbox runs as `choiros` system user (ADR-0020).
5. **Route pointers**: Traffic routing per user — `main` → live, `dev` → dev, custom → branch. Stored in hypervisor SQLite.

### 4.5 Storage

Each VM gets:
- **data.img** (2GB virtio-blk): Mutable sandbox state. Mounted at `/opt/choiros/data/sandbox/`.
- **Nix store**: Read-only erofs disk (shared across VMs). With pmem transport: zero-copy DAX reads via EPT mapping.
- **No virtiofs** (ADR-0018): All shares removed for KSM page deduplication. Gateway token injected via kernel cmdline instead.

### 4.6 Machine Classes (ADR-0014 Phase 6)

Defined in `/etc/choiros/machine-classes.toml`:
- Each class specifies: hypervisor (cloud-hypervisor/firecracker), transport (pmem/blk), vCPU, memory, runner nix store path, systemd template
- Users can select preferred machine class via `/profile/machine-class`
- Admin can override per-user via `/admin/sandboxes/{user_id}/machine-class`

---

## 5. Desktop / UI

### 5.1 What the Dioxus Desktop Renders

A **desktop metaphor UI** with:
- **Window manager**: Draggable, resizable, minimizable, maximizable floating windows with z-ordering
- **Taskbar**: Shows open windows, allows switching
- **App launcher**: Available apps (writer, terminal, files, logs, settings, trace, viewer)

### 5.2 Window Types / Apps

| App ID | Component | Purpose |
|--------|-----------|---------|
| `writer` | `WriterView` | Rich document editor with live agent collaboration, overlay rendering, version history |
| `terminal` | `TerminalView` | xterm.js terminal connected via WebSocket (`/ws/terminal/{id}`) |
| `files` | `FilesView` | File browser for sandbox filesystem |
| `logs` | `LogsView` | Event log viewer (streamed via WebSocket) |
| `settings` | `SettingsView` | User preferences, model configuration |
| `trace` | `TraceView` | LLM call trace viewer |
| `viewer` | `ViewerShell` | Generic content viewer (text, images) |

### 5.3 Data Flow

1. **Initial state**: Frontend fetches desktop state from `/desktop/{id}` on load
2. **Real-time updates**: WebSocket connection to `/ws` for desktop state changes, telemetry, writer run events
3. **API calls**: All mutations go through REST API (open window, create terminal, execute conductor task, etc.)
4. **Auth**: WebAuthn flow managed by `AuthModal` component. Session cookie set by hypervisor.

### 5.4 BIOS Boot Screen

The frontend includes a retro BIOS-style boot animation (in the Nix-built index.html) that shows system initialization progress before the WASM app loads.

---

## 6. Persistence

### 6.1 Databases

| Database | Location | Schema | Purpose |
|----------|----------|--------|---------|
| **hypervisor.db** | `data/hypervisor.db` | users, credentials, sessions, route_pointers, runtime_events, jobs, promotions | Hypervisor state |
| **events.db** | `/opt/choiros/data/sandbox/events.db` (in VM) or `data/events.db` (local) | events (seq, event_id, timestamp, event_type, payload, actor_id, user_id, session_id, thread_id) | Append-only event log — foundation of the actor system |
| **sandbox_{user}_{role}.db** | `data/sandbox_*.db` | Same events schema | Per-sandbox databases for branch/feature sandboxes |
| **memory store** | Per-user SQLite (rusqlite) | user_inputs, version_snapshots, run_trajectories, doc_trajectories | MemoryActor symbolic retrieval |
| **cogent.db** | `.cogent/cogent.db` | Work items, attestations, notes, docs | cogent work management (Go binary) |
| **cogent-private.db** | `.cogent/cogent-private.db` (gitignored) | Private operational notes, SSH keys, credentials | cogent private data |

### 6.2 What State is Persisted

- **Events**: All actor interactions are logged as events in the append-only event store
- **Desktop state**: Window positions, sizes, app registrations — reconstructed from events
- **Documents**: `.qwy` format files on the sandbox filesystem, with version history and citation registry
- **Terminal sessions**: PTY processes are ephemeral (recreated on restart)
- **Run state**: Conductor run states are reconstructed from events on startup (`restore_run_states`)
- **Harness checkpoints**: `harness.checkpoint` events for crash recovery

### 6.3 Document Versioning (Writer)

The Writer uses a `WriterDocumentRuntime` with:
- **RunDocument**: In-memory document state with sections, metadata, version tree
- **Versions**: Each change creates a new `DocumentVersion` with: version_id, content, source (User/Writer/Conductor/Researcher/Terminal), parent_version_id, overlays
- **Overlays**: Proposed changes that sit atop a version (pending → accepted/rejected/dismissed)
- **Patch operations**: `PatchOp` with kinds: InsertAfter, InsertBefore, Replace, Delete, Append
- **Persistence**: Saved as `.qwy` files (TOML frontmatter + markdown content) on the sandbox filesystem

---

## 7. BAML Definitions

BAML (Boundary AI Markup Language) defines the structured LLM function contracts.

### 7.1 Client Definitions (`clients.baml`)

| Client Name | Provider | Model | Use Case |
|-------------|----------|-------|----------|
| `Orchestrator` | aws-bedrock | claude-opus-4-5 | Complex reasoning, orchestration decisions |
| `FastResponse` | anthropic (via z.ai) | glm-4.7 | Quick responses, high-volume tasks |
| `HunterAlpha` | openrouter | hunter-alpha | 1T param agentic model, 1M context |
| `HealerAlpha` | openrouter | healer-alpha | Omni-modal (vision/audio/text) |
| `Nemotron` | openrouter | nemotron-3-super-120b | Free tier |
| `Mercury` | inception | mercury-2 | Diffusion LLM, very fast generation |

All use `Exponential` retry policy (2 retries, 300ms base delay, 1.5x multiplier).

### 7.2 Function Definitions

| Function | Input | Output | Client | Purpose |
|----------|-------|--------|--------|---------|
| `Decide` | messages, context, available_tools | `AgentDecision` (tool_calls + message) | Orchestrator | Core agent loop decision |
| `QuickResponse` | user_message, history | string | FastResponse | Simple query response |
| `ConductorBootstrapAgenda` | raw_objective, available_capabilities | dispatch_capabilities, block_reason, rationale | Orchestrator | Initial capability dispatch |
| `ConductorDecide` | run_id, objective, document_path, last_error | `ConductorDecision` (action + args + reason) | Orchestrator | Mid-run orchestration decision |
| `ConductorRefineObjective` | raw_objective, context, target_capability | refined_objective, success_criteria, estimated_steps | Orchestrator | Objective refinement for workers |
| `SummarizeChangeset` | patch_id, before_content, after_content, ops_json, source | summary, impact_level, op_taxonomy | FastResponse | Marginalia changeset summaries |

### 7.3 Type Definitions (`types.baml`)

Core types: `Message`, `AgentDecision`, `ToolResult`, `StreamChunk`

Tool call types (discriminated union):
- `BashToolCall`, `WebSearchToolCall`, `FetchUrlToolCall`
- `FileReadToolCall`, `FileWriteToolCall`, `FileEditToolCall`
- `MessageWriterToolCall` (inter-actor communication)
- `FinishedToolCall` (signals completion)

Citation types: `Citation`, `CitationKind` (RetrievedContext, InlineReference, BuildsOn, Contradicts, Reissues)

### 7.4 ALM Harness (`alm.baml`)

The ALM (Actor Language Model) is the general execution mode:
- `ContextSource` — model selects what context to load each turn (MemoryQuery, Document, PreviousTurn, ToolOutput)
- `NextAction` — model controls topology (ToolCalls, Program/DAG, FanOut, Recurse, Complete, Block)
- `DagStep` — execution DAG nodes with ops (ToolCall, LlmCall, Bash, MapReduce, Conditional, Assign)
- `AlmTurn` — complete turn output (sources + working_memory + next_action)

---

## 8. Build / Run / Test

### 8.1 Build System

- **Nix flake** (`flake.nix`): Defines all packages, NixOS configurations, and VM images
- **Crane** for Rust builds (cached dependency builds)
- **Workspace**: `hypervisor`, `sandbox`, `shared-types` in one Cargo workspace; `dioxus-desktop` is excluded (separate Cargo.lock for WASM target)
- **SQLX_OFFLINE=true**: SQL queries verified at build time via `.sqlx/` directory

### 8.2 Key Packages (Nix)

| Package | Description |
|---------|-------------|
| `sandbox` | Sandbox binary (runs inside VMs) |
| `hypervisor` | Hypervisor binary (runs on host) |
| `frontend` | WASM frontend (wasm-bindgen + wasm-opt) |
| `runtime-ctl` | OVH runtime control script |
| `cogent` | Go binary built from cogent-src flake input |

### 8.3 NixOS Configurations

| Config | System | Purpose |
|--------|--------|---------|
| `choiros-a`, `choiros-b` | x86_64-linux | OVH bare metal host configs (node A = prod, node B = staging) |
| `choiros-vfkit-user` | aarch64-linux | Local dev VM (macOS via vfkit) |
| `choiros-ch-sandbox-{live,dev}` | x86_64-linux | Cloud-hypervisor sandbox VMs (pmem/blk variants) |
| `choiros-fc-sandbox-{live,dev}` | x86_64-linux | Firecracker sandbox VMs (pmem/blk variants) |
| `choiros-worker-ch-{pmem,blk}` | x86_64-linux | Worker VMs (thick guest profile) |

### 8.4 Local Development

```bash
just dev                    # Build UI (release) + start vfkit cutover stack
just local-hypervisor       # Run hypervisor with local frontend dist
just dev-all               # tmux-backed: control plane + runtime plane
just vfkit-vm-runner       # Run vfkit guest VM (nix build + runner)
just vfkit-guest-shell     # SSH into guest VM
```

### 8.5 Testing

```bash
cargo test --workspace     # All unit + integration tests
just test-unit             # Unit tests only (--lib)
just test-integration      # Integration tests only
just test-sandbox-lib      # Fast scoped sandbox tests
just test-conductor-fast   # Conductor-specific tests
just test-e2e-vfkit-proof  # Playwright E2E (requires running dev stack)
just check                 # cargo fmt --check + cargo clippy
```

### 8.6 Key Scripts

| Script | Purpose |
|--------|---------|
| `scripts/dev-vfkit.sh` | Local dev stack management (start/stop/status/attach) |
| `scripts/sandbox-test.sh` | Fast scoped sandbox test runner |
| `scripts/generate-atlas.sh` | Regenerate docs/ATLAS.md from doc frontmatter |
| `scripts/generate-types.sh` | Generate TypeScript types from shared-types |
| `scripts/ops/ovh-runtime-ctl.sh` | OVH production VM lifecycle management |
| `scripts/ops/vfkit-runtime-ctl.sh` | macOS vfkit VM management |
| `scripts/ops/vfkit-reset.sh` | Reset stale vfkit VMs/tunnels/pid files |
| `scripts/ops/bootstrap-local-linux-builder.sh` | Bootstrap local aarch64-linux builder for Nix |
| `scripts/http/writer_api_smoke.sh` | Writer API smoke tests |
| `scripts/http/files_api_smoke.sh` | Files API smoke tests |

---

## 9. Integration Points for Go Rewrite

### 9.1 Critical Invariants

1. **Event sourcing is the foundation**: All state is derived from the append-only event log. The Go rewrite must preserve event-sourced architecture or provide an equivalent.

2. **Actor isolation**: Each actor handles messages sequentially. No shared mutable state. The ractor `call!`/`cast!` pattern must be preserved (Go channels or similar).

3. **Keyless sandbox boundary**: Sandboxes never hold LLM API keys. The provider gateway on the hypervisor injects secrets. This security boundary is non-negotiable.

4. **Document versioning model**: The writer's version tree (versions, overlays, patch operations) is the core user-facing primitive. Its semantics must be preserved.

5. **Structured agent loop**: The `AgentHarness` decide→execute→loop pattern with BAML contracts is the core execution model. The Go rewrite needs equivalent structured output parsing.

6. **Run state machine**: Conductor runs go through Queued → Running → WaitingWorker → Completed/Failed. Active calls, agenda items, artifacts, and decision logs are tracked.

### 9.2 External Interfaces

| Interface | Protocol | Must Preserve |
|-----------|----------|---------------|
| Hypervisor HTTP API | REST (JSON) | All auth, admin, provider gateway routes |
| Sandbox HTTP API | REST (JSON) | All desktop, terminal, writer, conductor, files routes |
| WebSocket protocols | JSON frames | Desktop WS, terminal WS (xterm.js), logs WS, writer run events |
| Provider gateway | HTTP proxy | Token auth, upstream allowlist, per-sandbox rate limiting, Bedrock rewrite |
| cogent integration | CLI subprocess | `cogent work ready --json`, `cogent work claim --json`, state dir in `.cogent/` |
| VM lifecycle | systemd / vfkit-runtime-ctl | boot, stop, hibernate, swap, branch operations |
| BAML contracts | LLM structured output | All function signatures and type contracts |
| Frontend static assets | HTTP file serving | index.html, WASM, JS, CSS |

### 9.3 State That Must Migrate

| State | Current Storage | Migration Path |
|-------|----------------|----------------|
| User accounts & credentials | hypervisor.db (SQLite) | Export/import or shared DB |
| Event log | events.db (SQLite) per sandbox | Replay or snapshot |
| Route pointers | hypervisor.db | Export/import |
| Document versions | .qwy files on sandbox filesystem | File format must be preserved or migrated |
| Memory collections | Per-user SQLite | Export/import |
| Sessions | hypervisor.db | Can be reset (users re-login) |
| Job queue | hypervisor.db | Can be drained before migration |
| cogent work graph | .cogent/cogent.db | Already Go — no migration needed |

### 9.4 Complexity Hotspots

These areas have the most complex logic and will need careful reimplementation:

1. **WriterActor** (`sandbox/src/actors/writer/mod.rs` — 2477 lines): Document runtime, version management, overlay system, delegation, inbox processing, orchestration
2. **ConductorActor runtime** (`sandbox/src/actors/conductor/runtime/` — 9 files): Run start, capability dispatch, completion handling, decision logic, durability, harness integration
3. **TerminalActor** (`sandbox/src/actors/terminal.rs` — 2361 lines): PTY management, agent harness integration, writer communication
4. **AgentHarness** (`sandbox/src/actors/agent_harness/` — mod.rs: 1776 lines, alm.rs: 1344 lines): Core agentic loop, tool execution, progress tracking, budget management
5. **SandboxRegistry** (`hypervisor/src/sandbox/mod.rs` — 1367 lines): VM lifecycle, port management, branch handling, memory pressure
6. **Provider Gateway** (`hypervisor/src/provider_gateway.rs` — 625 lines): Multi-provider routing, Bedrock rewrite, rate limiting, auth modes
7. **ApplicationSupervisor** (`sandbox/src/supervisor/mod.rs` — 1570 lines): Supervision tree, worker signal policy, escalation handling

### 9.5 What cogent Already Handles (No Rewrite Needed)

The `cogent` Go binary already provides:
- Work graph management (list, show, ready, claim)
- Attestation records
- Document management (doc-set, note-add)
- Autonomous dispatch (`cogent serve --auto`)
- Dashboard and UI

The Go rewrite should **unify** choiros-rs functionality into the cogent codebase, not replace cogent.

---

## Appendix: File Size Reference (Largest Source Files)

| File | Lines | Purpose |
|------|-------|---------|
| `shared-types/src/lib.rs` | ~2230 | All shared types |
| `sandbox/src/actors/writer/mod.rs` | ~2477 | Writer actor |
| `sandbox/src/actors/terminal.rs` | ~2361 | Terminal actor |
| `sandbox/src/actors/desktop.rs` | ~2108 | Desktop actor |
| `sandbox/src/actors/agent_harness/mod.rs` | ~1776 | Agent harness |
| `dioxus-desktop/src/api.rs` | ~1658 | Frontend API client |
| `sandbox/src/supervisor/mod.rs` | ~1570 | Application supervisor |
| `dioxus-desktop/src/desktop_window.rs` | ~1407 | Window management UI |
| `hypervisor/src/sandbox/mod.rs` | ~1367 | Sandbox registry |
| `sandbox/src/actors/conductor/state.rs` | ~1298 | Conductor run state |
