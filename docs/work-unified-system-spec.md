# Unified Multiagent System — Spec Sketch

> **Date**: 2026-04-08
> **Status**: Draft — design sketch for review (revised after design review)
> **Sources**: choiros-rs architecture analysis, cogent architecture analysis, Dolt integration research, Go agent runtime ecosystem research

---

## 1. System Identity

The unified system is a **single multiagent operating system**, written entirely in Go, that subsumes both ChoirOS (Rust) and Cogent (Go) into one coherent runtime. It provides an OS layer (agent runtime, scheduler, persistence, VM management, provider gateway) and an app layer (appagent-driven applications with canonical editing). Users interact through a web desktop and a programmatic API. Agents interact through the same API and through direct Go interfaces (in-process) or HTTP calls (remote). The system runs on bare-metal Linux hosts, isolates untrusted code execution in Firecracker microVMs, and manages LLM API access through a centralized provider gateway that holds all secrets. Name TBD — referred to as "the system" throughout this document.

The implementation extends the existing **cogent repository** (`/Users/wiz/cogent`) directly. No new repo, no module indirection. The cogent codebase IS the unified system codebase.

---

## 2. Architecture Overview

### 2.1 High-Level Component Diagram

```
┌────────────────────────────────────────────────────────────────────────────────┐
│                              WEB DESKTOP (Browser)                             │
│                                                                                │
│   Svelte SPA (reactive UI)  │  Pretext (text layout)  │  WebSocket / SSE      │
└───────────────────────────┬────────────────────────────────────────────────────┘
                            │  JSON API / WS / SSE
                            ▼
┌────────────────────────────────────────────────────────────────────────────────┐
│                              UNIFIED GO PROCESS                                │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │                           OS LAYER                                       │  │
│  │                                                                          │  │
│  │  ┌───────────────┐  ┌──────────────┐  ┌──────────────┐  ┌────────────┐  │  │
│  │  │ Desktop Shell │  │  Scheduler   │  │  Provider    │  │ Authority  │  │  │
│  │  │ (session mgmt,│  │  (internal   │  │  Gateway     │  │ & Lease    │  │  │
│  │  │  window mgmt, │  │   dispatch,  │  │  (LLM keys,  │  │ (VM iso,   │  │  │
│  │  │  app lifecycl)│  │   work graph)│  │   routing)   │  │  cap tkns) │  │  │
│  │  └───────┬───────┘  └──────┬───────┘  └──────┬───────┘  └─────┬──────┘  │  │
│  │          │                 │                  │                │          │  │
│  │  ┌───────┴─────────────────┴──────────────────┴────────────────┴───────┐  │  │
│  │  │                    Agent Runtime                                    │  │  │
│  │  │         (standardized contract for all agents)                      │  │  │
│  │  │                                                                     │  │  │
│  │  │   Identity │ Go Channels (local) │ HTTP (remote) │ Files │ Sessions│  │  │
│  │  └─────────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                          │  │
│  │  ┌─────────────────────────────────────────────────────────────────────┐  │  │
│  │  │                  Persistence Substrate                              │  │  │
│  │  │   SQLite (runtime state, events, sessions)  │  Dolt (e-text data)  │  │  │
│  │  └─────────────────────────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │                           APP LAYER                                      │  │
│  │                                                                          │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌───────────────┐  │  │
│  │  │  E-Text App │  │ Terminal App│  │  Files App  │  │  Future Apps  │  │  │
│  │  │             │  │             │  │             │  │               │  │  │
│  │  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │  │               │  │  │
│  │  │ │AppAgent │ │  │ │AppAgent │ │  │ │AppAgent │ │  │               │  │  │
│  │  │ │(writer) │ │  │ │(exec)   │ │  │ │(fs mgr) │ │  │               │  │  │
│  │  │ └────┬────┘ │  │ └────┬────┘ │  │ └────┬────┘ │  │               │  │  │
│  │  │      │      │  │      │      │  │      │      │  │               │  │  │
│  │  │ ┌────┴────┐ │  │ ┌────┴────┐ │  │             │  │               │  │  │
│  │  │ │Workers  │ │  │ │Workers  │ │  │             │  │               │  │  │
│  │  │ │(research│ │  │ │(sandbox │ │  │             │  │               │  │  │
│  │  │ │ draft)  │ │  │ │ cmds)   │ │  │             │  │               │  │  │
│  │  │ └─────────┘ │  │ └─────────┘ │  │             │  │               │  │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  └───────────────┘  │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
│                                                                                │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │                       MICROVM SUBSTRATE                                  │  │
│  │                                                                          │  │
│  │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────────┐   │  │
│  │  │ User Sandbox VM  │  │ User Sandbox VM  │  │ Worker Pool VMs      │   │  │
│  │  │ (per-user, live) │  │ (per-user, dev)  │  │ (shared, thick guest)│   │  │
│  │  │                  │  │                  │  │                      │   │  │
│  │  │ Agent processes  │  │ Agent processes  │  │ Agent processes      │   │  │
│  │  │ Tool executors   │  │ Tool executors   │  │ Tool executors       │   │  │
│  │  │ Vsock ↔ host     │  │ Vsock ↔ host     │  │ Vsock ↔ host        │   │  │
│  │  └──────────────────┘  └──────────────────┘  └──────────────────────┘   │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────────────────────────────┘

Bare Metal Host (NixOS, OVH)
```

### 2.2 Mapping Summary: Current → Unified

**From choiros-rs (Rust)**:
- Hypervisor (control plane, auth, proxy, provider gateway) → **OS Layer**: Authority & Lease, Provider Gateway, Desktop Shell (auth)
- Sandbox (actor system, event store, agents) → **OS Layer**: Agent Runtime, Persistence; **App Layer**: E-Text AppAgent, Terminal AppAgent
- Dioxus WASM frontend → **OS Layer**: Desktop Shell (replaced with Svelte SPA + Pretext for the e-text editor)
- BAML contracts → **Agent Runtime**: tool calling + structured output (replaced with Go-native structured output via cogent's existing LLM clients)
- ractor actor system → **Agent Runtime**: goroutine supervisor (replaced with plain goroutines + channels + custom lightweight supervisor)
- shared-types → eliminated (one language, one process, shared Go types directly)

**From cogent (Go)**:
- Work graph (SQLite, state machine) → **OS Layer**: Scheduler (internal records)
- Adapter system (claude, native) → **Agent Runtime**: adapter subsystem (preserved and extended)
- Native adapter (LLM tool loop, co-agents, channels) → **Agent Runtime**: core execution engine (preserved as the canonical agent loop)
- serve runtime (HTTP, WebSocket, supervisor) → **OS Layer**: Desktop Shell JSON API server (merged with hypervisor HTTP layer)
- CLI surface → preserved as the primary operator/automation interface
- Attestation model → **Scheduler**: internal quality gate (preserved)
- Mind-graph UI → **Desktop Shell**: one app among many (preserved, embedded via `embed.FS`)
- Capability tokens (Ed25519) → **Authority & Lease**: agent identity and authorization (preserved)
- EventBus → **OS Layer**: event distribution (preserved, extended to serve app events)
- Hand-rolled LLM clients (Anthropic, OpenAI) → **Agent Runtime**: LLM provider access (preserved, extended for new providers)
- ToolRegistry pattern → **Agent Runtime**: tool system (preserved as the canonical tool registration pattern)

**What gets dropped**:
- Dioxus WASM frontend (replaced by Svelte SPA with Pretext)
- ractor dependency (replaced by plain goroutines + channels + custom supervisor)
- BAML code generation (replaced by Go-native structured output)
- shared-types crate (unnecessary — single language)
- The entire choiros-rs/cogent boundary (two processes, CLI subprocess integration, gateway token dance)
- Separate hypervisor and sandbox processes (unified into one process per host)
- `.qwy` file format (replaced by Dolt-backed relational storage for e-text)

---

## 3. OS Layer Specification

### 3.1 Agent Runtime

**Purpose**: The one standardized contract that all agents — local goroutines, local processes, and remote VM-hosted processes — implement. This is the system's most important abstraction.

**What it subsumes**:
- choiros-rs: `AgentHarness`, `WorkerPort` trait, `ALM` harness, actor message types, BAML function contracts
- cogent: `adapterapi.Adapter`, `adapterapi.LiveAgentAdapter`, `adapterapi.LiveSession`, native adapter's tool loop, co-agent manager, channel manager

**Key interfaces**:

```go
// AgentCard describes an agent's capabilities.
type AgentCard struct {
    ID          string            `json:"id"`
    Name        string            `json:"name"`
    Description string            `json:"description"`
    Skills      []Skill           `json:"skills"`
    Endpoint    string            `json:"endpoint,omitempty"` // empty for local agents
    AuthScheme  string            `json:"auth_scheme,omitempty"`
}

// Agent is the universal contract. Every agent — appagent, worker, local, remote — implements this.
type Agent interface {
    Card() AgentCard

    // HandleTask processes a task and returns a result.
    // For local agents: direct Go function call.
    // For remote agents: HTTP call over vsock (transport hidden by the runtime).
    HandleTask(ctx context.Context, task Task) (TaskResult, error)

    // HandleMessage processes an inter-agent message (fire-and-forget or request-response).
    HandleMessage(ctx context.Context, msg Message) error

    // Status returns the agent's current operational status.
    Status(ctx context.Context) (AgentStatus, error)
}

// Task is the unit of work delegated between agents.
type Task struct {
    ID          string            `json:"id"`
    ParentID    string            `json:"parent_id,omitempty"`
    Objective   string            `json:"objective"`
    Input       []Part            `json:"input"`
    Constraints TaskConstraints   `json:"constraints,omitempty"`
}

// TaskResult is returned when a task completes or fails.
type TaskResult struct {
    TaskID  string     `json:"task_id"`
    Status  TaskStatus `json:"status"` // completed, failed, blocked
    Output  []Part     `json:"output"`
    Error   string     `json:"error,omitempty"`
}

// Part is a typed content chunk (text, file reference, structured data).
type Part struct {
    Kind    string          `json:"kind"` // "text", "file", "data", "artifact"
    Content json.RawMessage `json:"content"`
}

// Message is the coagent messaging primitive (inter-agent communication).
type Message struct {
    ID        string   `json:"id"`
    From      string   `json:"from"`       // sender agent ID
    To        string   `json:"to"`         // recipient agent ID
    Channel   string   `json:"channel"`    // optional named channel
    Parts     []Part   `json:"parts"`
    ReplyTo   string   `json:"reply_to,omitempty"`
}
```

**Tool access via ToolRegistry**:

Every agent gets access to tools through cogent's existing `ToolRegistry` pattern — plain Go functions registered in a map (same pattern as `internal/adapters/native/tools.go`). The tool surface depends on the agent's role (appagent vs. worker) and execution context (local vs. VM-sandboxed):

```go
// ToolSet defines what tools an agent has access to.
type ToolSet struct {
    FileRead     bool
    FileWrite    bool
    FileEdit     bool
    Bash         bool   // only in sandboxed contexts
    WebSearch    bool
    WebFetch     bool
    MessageAgent bool   // send messages to other agents
    Custom       []Tool // app-specific tools registered by the appagent
}
```

Workers get `MessageAgent` but NOT direct write access to canonical app state. They propose changes by messaging the appagent.

**Persistence model**: Agent sessions, turns, and events are persisted in SQLite (cogent's existing schema). The agent runtime does NOT own app-layer state — that belongs to each app's persistence layer.

**Local vs. remote**: The `Agent` interface is the same. For local agents (goroutines), `HandleTask` is a direct Go function call. For remote agents (in microVMs), the runtime wraps the call in an HTTP request over vsock — the same interface shape, just serialized. The caller never knows the difference.

### 3.2 Scheduler

**Purpose**: Internal execution machinery that decides what work to do, when, and with which resources. Explicitly NOT a user-facing ontology — users interact with apps, not with the scheduler.

**What it subsumes**:
- choiros-rs: `ConductorActor` (orchestration decisions, capability dispatch, run state machine), `self_directed_dispatch.rs` (cogent CLI integration)
- cogent: work graph (SQLite tables), state machine, claim/lease model, supervisor agent, auto-dispatch, rotation config, briefing/hydration, attestation gating

**Key design**: The scheduler is the *internal merge* of the choiros Conductor and the cogent work graph. From the outside (apps, users), you submit objectives to apps. The app's appagent decides whether to handle it directly or delegate. When delegation happens, the scheduler tracks it as an internal work item.

```go
// SchedulerRecord is an internal work tracking record.
// Users and external systems do NOT create these directly.
// They are created by appagents when they delegate work.
type SchedulerRecord struct {
    ID              string              `json:"id"`
    AppID           string              `json:"app_id"`           // which app owns this
    AgentID         string              `json:"agent_id"`         // which agent is assigned
    Objective       string              `json:"objective"`
    State           ExecutionState      `json:"state"`            // queued → running → completed/failed
    Priority        int                 `json:"priority"`
    Constraints     TaskConstraints     `json:"constraints"`
    AttemptEpoch    int                 `json:"attempt_epoch"`
    ClaimedBy       string              `json:"claimed_by,omitempty"`
    ClaimedUntil    *time.Time          `json:"claimed_until,omitempty"`
    CreatedAt       time.Time           `json:"created_at"`
    UpdatedAt       time.Time           `json:"updated_at"`
}

// ExecutionState tracks work lifecycle.
type ExecutionState string

const (
    StateQueued    ExecutionState = "queued"
    StateRunning   ExecutionState = "running"
    StateBlocked   ExecutionState = "blocked"
    StateCompleted ExecutionState = "completed"
    StateFailed    ExecutionState = "failed"
    StateCancelled ExecutionState = "cancelled"
)
```

**Dispatch logic** (from cogent's supervisor, preserved):
1. Scheduler monitors for queued work items
2. Selects adapter + model using rotation pool (round-robin with history-aware avoidance)
3. Hydrates briefing context via `ProjectHydrate()`
4. Dispatches to agent via the Agent Runtime
5. Monitors for stalls, handles completion/failure
6. Attestation gating: work is only `completed` when verification evidence satisfies policy

**Persistence**: SQLite (cogent's existing `work_items`, `work_edges`, `attestation_records`, `jobs`, `sessions`, `turns`, `events` tables). The scheduler's DB schema is the cogent schema, preserved as-is.

**What disappears**: The user-facing `cogent work` CLI commands remain for operator/automation use, but they become an internal debugging/ops surface. Normal users never see work items — they interact with apps.

### 3.3 Desktop Shell

**Purpose**: Session management, window management, app lifecycle, authentication, and the HTTP/WebSocket/SSE API surface that everything talks to.

**What it subsumes**:
- choiros-rs: `DesktopActor` (window state), Dioxus frontend (replaced), hypervisor HTTP server (auth, routing, admin), sandbox HTTP API (desktop, files, writer, conductor endpoints), WebSocket protocols
- cogent: `serve.go` HTTP server (merged), WebSocket hub (merged), embedded web UI (mind-graph becomes one app)

**Key interfaces**:

```go
// App is a registered application in the desktop.
type App struct {
    ID          string   `json:"id"`
    Name        string   `json:"name"`
    Icon        string   `json:"icon"`
    Description string   `json:"description"`
    AgentID     string   `json:"agent_id,omitempty"` // optional appagent
    Routes      []Route  `json:"routes"`             // HTTP routes this app owns
    HasUI       bool     `json:"has_ui"`             // renders in the desktop
}

// WindowState represents a window in the desktop.
type WindowState struct {
    ID       string  `json:"id"`
    AppID    string  `json:"app_id"`
    Title    string  `json:"title"`
    X, Y     float64 `json:"x,y"`
    W, H     float64 `json:"w,h"`
    ZIndex   int     `json:"z_index"`
    State    string  `json:"state"` // normal, minimized, maximized
}

// DesktopSession represents a user's active session.
type DesktopSession struct {
    ID       string        `json:"id"`
    UserID   string        `json:"user_id"`
    Windows  []WindowState `json:"windows"`
    ActiveWin string       `json:"active_window"`
}
```

**HTTP server** (unified — one server, not two):
- Auth routes: WebAuthn registration/login/logout/recovery (from choiros hypervisor)
- Desktop routes: session state, window CRUD (JSON API)
- App routes: each app registers its own routes under `/app/{app_id}/...` (JSON API)
- API routes: `/api/v1/...` — the programmatic JSON API for agents and external consumers
- Provider gateway: `/provider/v1/{provider}/{rest}` (from choiros hypervisor, preserved exactly)
- Admin: `/admin/...` (VM management, system status)
- Static assets: the embedded Svelte build (served via `embed.FS`, same pattern as cogent's mind-graph today)
- WebSocket: `/ws` (desktop events, app events — unified event stream)
- SSE: `/sse/...` (agent status streams, task progress)

The Go backend is a **pure JSON API + WebSocket server**. It does NOT render HTML. All rendering is handled client-side by the Svelte SPA.

**Web desktop** (replaces Dioxus WASM):
- **Svelte** reactive SPA — compiled, small runtime, excellent performance
- The Svelte app is a separate build artifact, **embedded in the Go binary** via `embed.FS` (same pattern as cogent's mind-graph today)
- **Pretext** (`@chenglou/pretext`) is used specifically for the e-text editor — high-performance text measurement and layout without DOM reflow
- Window management (drag, resize, minimize, maximize, z-ordering) is implemented in Svelte client-side
- Real-time: SSE for status streams, WebSocket for interactive sessions (terminal, agent chat)
- The Svelte app communicates with the Go backend exclusively via JSON API and WebSocket

**Persistence**: Desktop state (windows, sessions) in SQLite. Auth state (users, credentials, sessions) in SQLite (from choiros hypervisor schema, preserved).

### 3.4 Persistence Substrate

**Purpose**: Provide the appropriate storage backend for each kind of state. Not one database — the right tool for each job.

**What it subsumes**:
- choiros-rs: `events.db` (event store), `hypervisor.db` (auth, routes, jobs), `memory store` (symbolic memory), `.qwy` files (document storage)
- cogent: `cogent.db` (work graph, sessions, jobs, events, artifacts), `cogent-private.db` (private notes, credentials)

**Storage tiers**:

| Tier | Engine | Purpose | Schema Source |
|------|--------|---------|--------------|
| **Runtime DB** | SQLite | Agent sessions, jobs, turns, events, scheduler records, auth, desktop state | cogent's 21-table schema + choiros hypervisor schema (merged) |
| **Private DB** | SQLite (gitignored) | Credentials, private notes, CA keys | cogent's private_notes (preserved) |
| **E-Text DB** | Dolt (embedded, per-user) | Document content, versioned with full provenance | New schema (see §5) |
| **Filesystem** | Local disk / VM data.img | Agent artifacts, raw outputs, native session history, config | cogent's `.cogent/` layout (preserved) |

**Key decisions**:
- **SQLite for runtime state**: `modernc.org/sqlite` (pure Go, no CGo). WAL mode, `_txlock=immediate`, `MaxOpenConns=1`. Same configuration as current cogent.
- **Dolt for e-text (and potentially other versioned-data apps)**: Embedded via `dolthub/driver`. Per-user database directories. Full version control via SQL.
- **No event store actor**: The choiros-rs pattern of an `EventStoreActor` wrapping SQLite is unnecessary — Go's `database/sql` with proper transaction handling provides the same sequential write guarantee.
- **Event log preserved**: Append-only events table from cogent is the canonical audit trail. All significant state changes emit events.

**SQLite schema unification**: The unified runtime DB merges:
- cogent's 21 tables (sessions, jobs, turns, events, work_items, work_edges, attestation_records, etc.)
- choiros hypervisor's tables (users, credentials, sessions/cookies, route_pointers, runtime_events)
- Desktop state tables (windows, app registrations)

Migration: additive schema evolution (CREATE TABLE IF NOT EXISTS), same as cogent today.

### 3.5 Authority & Lease Model

**Purpose**: Security boundary enforcement. Who can do what, where, and for how long.

**What it subsumes**:
- choiros-rs: keyless sandbox policy, gateway token, provider gateway auth, VM lifecycle/isolation, non-root sandbox user, route pointers, machine classes
- cogent: Ed25519 CA, capability tokens, agent credentials, session locks

**Key invariants**:
1. **MicroVMs never hold LLM API keys** (choiros invariant, preserved). The provider gateway on the host injects secrets.
2. **Agents authenticate via Ed25519 capability tokens** (cogent invariant, preserved). Tokens are time-limited, role-scoped, and signed by the host CA.
3. **One user per sandbox VM** — singular authority. No multi-tenant VMs.
4. **Workers cannot directly mutate canonical app state** — enforced by the agent runtime's tool set (workers get `MessageAgent`, not direct state write).

**Capability token model** (from cogent, preserved):

```go
// CapabilityToken authorizes an agent for specific actions.
type CapabilityToken struct {
    TokenID   string    `json:"token_id"`
    AgentID   string    `json:"agent_id"`
    Role      string    `json:"role"`     // "appagent", "worker", "supervisor"
    Scope     []string  `json:"scope"`    // allowed actions
    IssuedAt  time.Time `json:"issued_at"`
    ExpiresAt time.Time `json:"expires_at"`
    Signature []byte    `json:"signature"` // Ed25519 signature
}
```

**MicroVM lifecycle** (from choiros, managed directly by Go via firecracker-go-sdk):

| VM Type | Purpose | Guest Profile | Management |
|---------|---------|---------------|------------|
| User Sandbox (live) | Per-user agent execution | Minimal (2 vCPU, 1GB) | Go → firecracker-go-sdk → API socket |
| User Sandbox (dev) | Dev/branch sandbox | Minimal | Go → firecracker-go-sdk → API socket |
| Worker VM | Shared pool, thick tooling | Worker (more resources) | Go → firecracker-go-sdk → API socket |

The Go binary IS the hypervisor. VM lifecycle (boot, stop, hibernate, idle watchdog, memory pressure) is managed **directly via firecracker-go-sdk API socket calls** — no systemd templates, no shell scripts in between. Nix builds the VM images (NixOS guest configs, kernel, disk images via microvm.nix), but the Go process manages everything at runtime.

VM lifecycle: boot → running → (idle timeout) → hibernated/stopped. Idle watchdog (30s scan, configurable timeout). Memory pressure check before spawn.

**Host ↔ Guest IPC**: vsock (preferred — no network config) or virtio-net (for compatibility). Agent processes inside VMs communicate with the host via HTTP over vsock — the same JSON API shape as in-process calls, just serialized over a different transport.

### 3.6 Provider Gateway

**Purpose**: Centralized LLM API key management and multi-provider routing. The one place where secrets live.

**What it subsumes**:
- choiros-rs: `provider_gateway.rs` (Anthropic, OpenAI, Z.AI, Kimi, Inception, OpenRouter, Tavily, Brave, Exa, AWS Bedrock proxying, per-sandbox rate limiting, Bedrock request rewriting)
- cogent: native adapter's provider configuration (ZAI, Bedrock, ChatGPT, direct Anthropic, direct OpenAI), web search API key management (Exa, Tavily, Brave, Serper)

**Design**: The provider gateway is now part of the unified OS process (not a separate hypervisor). It serves the same role — agents send LLM requests to a local endpoint, the gateway injects the real API key and proxies to the upstream provider.

```go
// ProviderGateway routes LLM API calls to upstream providers,
// injecting API keys and enforcing rate limits.
type ProviderGateway struct {
    providers map[string]ProviderConfig
    rateLimiter *RateLimiter
}

// ProviderConfig defines an upstream LLM provider.
type ProviderConfig struct {
    Name       string   // "anthropic", "openai", "bedrock", "zai", etc.
    BaseURL    string
    AuthHeader string   // e.g., "x-api-key", "Authorization"
    APIKey     string   // loaded from env/config, NEVER exposed to agents
    Models     []string // allowed models
    RateLimit  RateLimit
}
```

**For local agents** (in the same process): the gateway is called directly via Go function call. No HTTP hop. LLM calls use cogent's existing hand-rolled streaming clients (`client_anthropic.go`, `client_openai.go`), extended for new providers as needed.

**For VM-hosted agents**: the gateway is called via HTTP from inside the VM, same as choiros-rs today. The gateway token is injected via VM kernel cmdline or vsock channel.

**Unified provider list** (merged from both systems):
- Anthropic (Claude Opus, Sonnet, Haiku)
- OpenAI (GPT-5.x)
- AWS Bedrock (Claude variants)
- Z.AI (GLM models)
- OpenRouter (aggregator)
- Inception (Mercury)
- Kimi (Moonshot)
- Google (Gemini)
- Web search: Exa, Tavily, Brave, Serper (round-robin rotation from cogent, preserved)

### 3.7 Goroutine Supervisor

**Purpose**: OTP-like supervision for agent goroutines without a framework dependency. The supervision quality comes from the pattern, not from an external library.

**Design**: A custom lightweight supervisor built on plain goroutines, Go channels, and context cancellation.

```go
// Supervisor manages a group of child goroutines with restart strategies.
type Supervisor struct {
    name       string
    strategy   RestartStrategy
    children   []*Child
    healthTick time.Duration
    ctx        context.Context
    cancel     context.CancelFunc
}

// RestartStrategy determines how failures are handled.
type RestartStrategy string

const (
    RestartOne RestartStrategy = "restart_one" // restart only the failed child
    RestartAll RestartStrategy = "restart_all" // restart all children when one fails
)

// Child represents a supervised goroutine.
type Child struct {
    Name      string
    Start     func(ctx context.Context) error // the goroutine's main function
    Health    chan struct{}                    // heartbeat channel
    done      chan error                       // signals completion/failure
}
```

**Key patterns**:
- **Parent monitors children via channels**: Each child goroutine sends on its `done` channel when it exits (with nil for clean shutdown, error for failure). The parent `select`s on all children's channels.
- **Restart strategies**: `restart_one` restarts only the failed child (appropriate for independent agents). `restart_all` restarts the entire supervision group (appropriate for agents with shared state).
- **Health checks**: Children periodically send on a heartbeat channel. The supervisor detects stalls when heartbeats stop (configurable timeout).
- **Graceful shutdown via context cancellation**: The supervisor's context is derived from the parent context. Cancelling the parent context cascades to all children. Children check `ctx.Done()` and clean up.
- **Supervision tree**: Supervisors can be children of other supervisors, forming a tree. The root supervisor is the main process.

```go
// Example: supervisor for e-text app agents
func NewETextSupervisor(ctx context.Context) *Supervisor {
    sup := &Supervisor{
        name:       "etext-supervisor",
        strategy:   RestartOne,
        healthTick: 10 * time.Second,
    }
    sup.AddChild(&Child{
        Name:  "etext-appagent",
        Start: runETextAppAgent,
    })
    sup.AddChild(&Child{
        Name:  "etext-worker-pool",
        Start: runETextWorkerPool,
    })
    return sup
}

func (s *Supervisor) Run(ctx context.Context) error {
    s.ctx, s.cancel = context.WithCancel(ctx)
    defer s.cancel()

    // Start all children
    for _, child := range s.children {
        s.startChild(child)
    }

    // Monitor loop
    for {
        select {
        case <-s.ctx.Done():
            return s.shutdownAll()
        case err := <-s.anyChildDone():
            child := s.identifyFailedChild(err)
            if s.strategy == RestartAll {
                s.restartAll()
            } else {
                s.restartChild(child)
            }
        }
    }
}
```

This is simple Go — no framework needed. The OTP-like quality comes from the disciplined application of the supervision pattern.

---

## 4. App Layer Specification

### 4.1 App Lifecycle and Registration

An **app** is a named unit of functionality with an optional UI, optional appagent, and a set of API routes. Apps are registered with the Desktop Shell at startup or dynamically.

```go
// AppDefinition is the static declaration of an app.
type AppDefinition struct {
    ID          string        `json:"id"`          // unique identifier, e.g., "etext"
    Name        string        `json:"name"`        // display name, e.g., "E-Text"
    Icon        string        `json:"icon"`
    Description string        `json:"description"`
    HasAgent    bool          `json:"has_agent"`   // whether this app has an appagent
    AgentConfig *AgentConfig  `json:"agent_config,omitempty"`
    APIPrefix   string        `json:"api_prefix"`  // e.g., "/app/etext"
}

// AppInstance is a running instance of an app for a specific user.
type AppInstance struct {
    AppID    string        `json:"app_id"`
    UserID   string        `json:"user_id"`
    AgentRef *AgentRef     `json:"agent_ref,omitempty"` // reference to the live appagent
    State    AppState      `json:"state"`               // starting, running, stopped
}
```

**Lifecycle**:
1. **Register**: App provides its `AppDefinition` to the Desktop Shell at system boot or via admin API
2. **Instantiate**: When a user opens the app, the Desktop Shell creates an `AppInstance`, optionally spawning the appagent
3. **Run**: The Svelte SPA renders the app's UI client-side, the appagent handles agentic requests, the JSON API routes are live
4. **Stop**: The app instance is torn down when the user closes it (or on session end); appagent is stopped, resources released

**App registry** (built-in apps for v1):

| App ID | Name | Has Agent | Description |
|--------|------|-----------|-------------|
| `etext` | E-Text | Yes | Versioned document editor (reference implementation) |
| `terminal` | Terminal | Yes | Interactive terminal / code execution |
| `files` | Files | No (or minimal) | File browser for sandbox filesystem |
| `mindgraph` | Mind Graph | No | Work graph Poincaré disk visualization (from cogent) |
| `settings` | Settings | No | User preferences, model config |
| `logs` | Logs | No | Event log viewer |

### 4.2 AppAgent Contract

An appagent is an `Agent` (§3.1) with elevated privileges: it is a **canonical editor** of its app's state. The appagent is the sole agent-side authority over the app's data.

```go
// AppAgent extends Agent with app-specific lifecycle methods.
type AppAgent interface {
    Agent

    // Init is called when the app instance starts. The appagent sets up
    // its internal state, connects to its persistence layer, etc.
    Init(ctx context.Context, appCtx AppContext) error

    // HandleUserAction processes a user's request to the app.
    // This is the entry point for "user asks the app to do something via agent."
    HandleUserAction(ctx context.Context, action UserAction) (ActionResult, error)

    // Shutdown is called when the app instance stops.
    Shutdown(ctx context.Context) error
}

// AppContext provides the appagent with access to OS services.
type AppContext struct {
    AppID       string
    UserID      string
    Scheduler   SchedulerClient   // to delegate work
    Runtime     RuntimeClient     // to spawn/manage workers
    Gateway     GatewayClient     // to make LLM calls
    Persistence PersistenceClient // to access the app's storage
    EventBus    EventBusClient    // to publish/subscribe events
}

// UserAction represents a user's request to the app via the agent.
type UserAction struct {
    ID        string `json:"id"`
    Kind      string `json:"kind"`    // "prompt", "edit", "command", etc.
    Payload   any    `json:"payload"`
}
```

**What an appagent can do**:
- Read and write its app's canonical state (e.g., e-text documents in Dolt)
- Delegate subtasks to workers via the Scheduler
- Spawn worker agents (local or remote) via the Agent Runtime
- Make LLM calls via the Provider Gateway (using cogent's native Anthropic/OpenAI streaming clients)
- Publish events to the EventBus
- Register custom tools for its workers via the ToolRegistry

**What an appagent cannot do**:
- Access another app's state directly (app isolation)
- Bypass the provider gateway (no direct LLM API keys)
- Create user accounts or modify auth state (OS-level operations)

### 4.3 Worker Agent Contract

A worker is an `Agent` with restricted privileges: it is a **subordinate non-canonical executor**. Workers do real work (research, code generation, analysis) but they cannot directly modify canonical app state.

```go
// WorkerConfig defines how a worker is spawned.
type WorkerConfig struct {
    AgentCard   AgentCard        `json:"agent_card"`
    ToolSet     ToolSet          `json:"tool_set"`
    Execution   ExecutionMode    `json:"execution_mode"` // "local", "vm_sandboxed"
    Budget      WorkerBudget     `json:"budget"`         // step limit, token limit, time limit
}

// WorkerBudget constrains worker execution.
type WorkerBudget struct {
    MaxSteps    int           `json:"max_steps"`
    MaxTokens   int           `json:"max_tokens"`
    MaxDuration time.Duration `json:"max_duration"`
}

// ExecutionMode determines where the worker runs.
type ExecutionMode string

const (
    ExecutionLocal      ExecutionMode = "local"        // goroutine in the host process
    ExecutionVMSandbox  ExecutionMode = "vm_sandboxed" // inside a microVM
)
```

**What a worker can do**:
- Execute tools from its authorized `ToolSet` (file read, bash, web search, etc.)
- Send messages/proposals to its appagent via `MessageAgent` tool
- Read (but not write) canonical app state if the appagent exposes it as a read-only resource

**What a worker cannot do**:
- Directly write to canonical app state (enforced by ToolSet — no direct Dolt/DB access)
- Spawn other agents (only appagents can delegate)
- Access the provider gateway directly (worker LLM calls are mediated by the agent runtime)

### 4.4 Canonical Editing: Users and AppAgents as Peers

This is a core design principle. Both users and appagents are **canonical editors** — their edits have equal authority and create canonical new versions.

**How it works for e-text** (the pattern all apps should follow):

```
User (via Svelte UI) ──────────┐
                                ├──→ E-Text Canonical State (Dolt) ──→ Version N+1
AppAgent (via agent loop) ─────┘

Workers ──→ Messages/Proposals ──→ AppAgent ──→ E-Text Canonical State ──→ Version N+1
```

1. **User edits**: User modifies content in the Svelte editor. The edit goes via JSON API to the Go backend, then directly to the app's persistence layer (e.g., Dolt commit with user as author). This creates a new canonical version. No agent involvement required.

2. **AppAgent edits**: AppAgent processes a user prompt, decides on changes, and writes to the app's persistence layer (e.g., Dolt commit with agent as author). This creates a new canonical version. Equivalent authority to user edits.

3. **Worker proposals**: Workers cannot commit directly. They send structured messages/proposals to the appagent via the `MessageAgent` tool. The appagent reviews and applies (or rejects) the proposal, creating a canonical version attributed to the appagent.

**Concurrency model (v1)**: Single-user, serialized writes on the `main` branch. The user and the appagent take turns — there is no concurrent editing. Real-time collaborative editing (CRDT/OT) and branch-based isolation for concurrent user+agent edits are deferred to a later version.

### 4.5 API Exposure Pattern

Every app exposes a JSON API that the Svelte frontend and external agents consume:

```
/app/{app_id}/api/...     → App-specific JSON API
/app/{app_id}/ws          → App-specific WebSocket (optional)
/app/{app_id}/sse         → App-specific SSE stream (optional)
```

All endpoints return JSON. The Svelte SPA handles all rendering client-side.

Example for e-text:
```
GET    /app/etext/api/documents                    → list documents (JSON)
POST   /app/etext/api/documents                    → create document (JSON)
GET    /app/etext/api/documents/{id}               → get document (JSON, current version)
GET    /app/etext/api/documents/{id}?at={commit}   → get document (JSON, historical version)
PUT    /app/etext/api/documents/{id}               → update document (user edit → Dolt commit)
GET    /app/etext/api/documents/{id}/history       → version history (Dolt log, JSON)
GET    /app/etext/api/documents/{id}/diff?from=&to= → diff between versions (JSON)
POST   /app/etext/api/documents/{id}/prompt        → submit prompt to appagent
GET    /app/etext/api/documents/{id}/blame         → blame (who edited what, JSON)
```

---

## 5. E-Text App (Reference Implementation)

### 5.1 Overview

The e-text app (formerly "writer") is the first and primary app. It demonstrates the full app model: Dolt-backed versioned storage, canonical editing by users and appagent, worker proposals, and a rich JSON API. The editor UI is built with Svelte and uses **Pretext** (`@chenglou/pretext`) for high-performance text measurement and layout without DOM reflow.

### 5.2 Dolt-Backed Versioned Storage

**Storage location**: `~/.choiros/users/{user_id}/etext/.dolt/` — one Dolt database per user.

**Connection** (embedded mode, per §3 of Dolt research):

```go
import (
    "database/sql"
    embedded "github.com/dolthub/driver"
)

func OpenETextDB(userID string) (*sql.DB, error) {
    dbPath := filepath.Join(choirosHome, "users", userID, "etext")
    dsn := fmt.Sprintf("file://%s?commitname=System&commitemail=system@choiros.local&database=etext",
        dbPath)
    cfg, err := embedded.ParseDSN(dsn)
    if err != nil {
        return nil, err
    }
    connector, err := embedded.NewConnector(cfg)
    if err != nil {
        return nil, err
    }
    return sql.OpenDB(connector), nil
}
```

**Schema**:

```sql
CREATE TABLE documents (
    doc_id      VARCHAR(36) DEFAULT (UUID()) PRIMARY KEY,
    title       VARCHAR(512) NOT NULL,
    doc_type    VARCHAR(64) NOT NULL DEFAULT 'text',
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE content (
    doc_id        VARCHAR(36) NOT NULL,
    section_id    VARCHAR(36) DEFAULT (UUID()),
    section_order INT NOT NULL,
    heading       VARCHAR(256),
    body          LONGTEXT NOT NULL,
    content_hash  VARCHAR(64),  -- SHA-256 for quick equality checks
    PRIMARY KEY (doc_id, section_id),
    INDEX idx_doc_order (doc_id, section_order),
    FOREIGN KEY (doc_id) REFERENCES documents(doc_id)
);

CREATE TABLE citations (
    citation_id   VARCHAR(36) DEFAULT (UUID()) PRIMARY KEY,
    doc_id        VARCHAR(36) NOT NULL,
    section_id    VARCHAR(36),
    source_url    VARCHAR(2048),
    source_title  VARCHAR(512),
    citation_kind VARCHAR(64) NOT NULL, -- 'retrieved', 'inline_ref', 'builds_on', 'contradicts'
    context_text  TEXT,
    created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (doc_id) REFERENCES documents(doc_id)
);

CREATE TABLE metadata (
    doc_id     VARCHAR(36) NOT NULL,
    meta_key   VARCHAR(128) NOT NULL,
    meta_value TEXT,
    PRIMARY KEY (doc_id, meta_key),
    FOREIGN KEY (doc_id) REFERENCES documents(doc_id)
);
```

**UUID primary keys** throughout (recommended by Dolt for merge-friendliness — no auto-increment conflicts across branches).

**Content split into sections** for granular cell-level diffs. Each section is independently diffable and mergeable by Dolt.

### 5.3 Version Creation

Every meaningful edit creates a Dolt commit with explicit author attribution:

```go
// SaveUserEdit creates a canonical version attributed to the user.
func (s *ETextStore) SaveUserEdit(ctx context.Context, req UserEditRequest) (string, error) {
    tx, err := s.db.BeginTx(ctx, nil)
    if err != nil {
        return "", err
    }
    defer tx.Rollback()

    // Apply the edit
    _, err = tx.ExecContext(ctx,
        `UPDATE content SET body = ?, content_hash = SHA2(?, 256), updated_at = NOW()
         WHERE doc_id = ? AND section_id = ?`,
        req.NewContent, req.NewContent, req.DocID, req.SectionID)
    if err != nil {
        return "", err
    }

    // Update document timestamp
    _, err = tx.ExecContext(ctx,
        `UPDATE documents SET updated_at = NOW() WHERE doc_id = ?`, req.DocID)
    if err != nil {
        return "", err
    }

    // Dolt: stage and commit
    _, err = tx.ExecContext(ctx, `CALL dolt_add('.')`)
    if err != nil {
        return "", err
    }

    var commitHash string
    err = tx.QueryRowContext(ctx,
        `CALL dolt_commit('-m', ?, '--author', ?)`,
        fmt.Sprintf("User edit: %s §%s", req.DocID, req.SectionID),
        fmt.Sprintf("%s <%s>", req.UserName, req.UserEmail),
    ).Scan(&commitHash)
    if err != nil {
        return "", err
    }

    return commitHash, tx.Commit()
}

// SaveAppAgentEdit creates a canonical version attributed to the appagent.
func (s *ETextStore) SaveAppAgentEdit(ctx context.Context, req AgentEditRequest) (string, error) {
    // Same pattern, but author is the appagent:
    // --author "ETextAgent <etext-agent@choiros.local>"
    // ... (analogous to SaveUserEdit)
}
```

### 5.4 AppAgent Behavior

The e-text appagent is the **sole agent-side canonical writer**. It:

1. Receives user prompts via `HandleUserAction`
2. Plans what changes are needed (LLM call via provider gateway, using cogent's native Anthropic/OpenAI clients)
3. Optionally delegates research/analysis to workers
4. Applies changes to Dolt (creating a new commit as the appagent author)
5. Publishes events so the Svelte UI updates in real-time via WebSocket/SSE

```go
type ETextAppAgent struct {
    store     *ETextStore       // Dolt-backed storage
    scheduler SchedulerClient   // for delegating to workers
    runtime   RuntimeClient     // for spawning workers
    gateway   GatewayClient     // for LLM calls
    eventBus  EventBusClient    // for real-time updates
}

func (a *ETextAppAgent) HandleUserAction(ctx context.Context, action UserAction) (ActionResult, error) {
    switch action.Kind {
    case "prompt":
        return a.handlePrompt(ctx, action.Payload)
    case "edit":
        // User direct edit — goes straight to Dolt, bypasses agent
        return a.handleDirectEdit(ctx, action.Payload)
    default:
        return ActionResult{}, fmt.Errorf("unknown action kind: %s", action.Kind)
    }
}

func (a *ETextAppAgent) handlePrompt(ctx context.Context, payload any) (ActionResult, error) {
    prompt := payload.(PromptPayload)

    // 1. Decide what to do (LLM call)
    decision, err := a.decideAction(ctx, prompt)
    if err != nil {
        return ActionResult{}, err
    }

    // 2. If research needed, delegate to worker
    if decision.NeedsResearch {
        task := Task{
            Objective: decision.ResearchObjective,
            Input:     []Part{{Kind: "text", Content: json.RawMessage(prompt.Text)}},
        }
        // Scheduler tracks this internally
        a.scheduler.SubmitTask(ctx, task)
    }

    // 3. Apply edits to Dolt (canonical appagent edit)
    for _, edit := range decision.Edits {
        _, err := a.store.SaveAppAgentEdit(ctx, edit)
        if err != nil {
            return ActionResult{}, err
        }
        // Publish real-time update
        a.eventBus.Publish(ctx, Event{
            Kind: "etext.section.updated",
            Data: edit,
        })
    }

    return ActionResult{Status: "completed"}, nil
}
```

### 5.5 Worker Interaction Model

Workers **propose** changes. They cannot commit to Dolt directly.

```go
// Worker tool: message_appagent
// Workers use this to send proposals to the appagent.
type EditProposal struct {
    DocID       string `json:"doc_id"`
    SectionID   string `json:"section_id"`
    ProposedBody string `json:"proposed_body"`
    Rationale    string `json:"rationale"`
    Citations    []CitationRef `json:"citations,omitempty"`
}
```

The worker sends an `EditProposal` message to the appagent. The appagent receives it, evaluates it (possibly with an LLM call), and either:
- **Accepts**: applies the edit as an appagent commit with a message referencing the worker's contribution
- **Rejects**: discards the proposal (optionally with feedback to the worker)
- **Modifies**: applies a modified version of the proposal

### 5.6 User Interaction Model

Users interact with e-text through the Svelte SPA:

1. **Direct editing**: User types in the Svelte-based editor (using Pretext for text measurement/layout) → content goes to the JSON API → Dolt commit with user author. No agent involvement. Canonical.

2. **Prompting**: User submits a natural language prompt → appagent processes it → appagent edits Dolt. User sees changes in real-time via SSE/WebSocket in the Svelte UI.

3. **Version history**: User browses commit history (Dolt log), views diffs between versions, reverts to previous versions. All via standard Dolt SQL queries exposed through the JSON API, rendered by Svelte.

4. **Blame**: User can see who (user or agent) last edited each section, via `dolt_blame_content`.

### 5.7 API Surface

```
# Documents
GET    /app/etext/api/documents                          → list all documents
POST   /app/etext/api/documents                          → create new document
GET    /app/etext/api/documents/{id}                     → get document with all sections
PUT    /app/etext/api/documents/{id}                     → update document metadata
DELETE /app/etext/api/documents/{id}                     → delete document

# Content (sections)
GET    /app/etext/api/documents/{id}/sections            → list sections
PUT    /app/etext/api/documents/{id}/sections/{sid}      → update section (user edit → Dolt commit)
POST   /app/etext/api/documents/{id}/sections            → add section
DELETE /app/etext/api/documents/{id}/sections/{sid}      → delete section

# Versioning
GET    /app/etext/api/documents/{id}/history             → commit log (dolt_log)
GET    /app/etext/api/documents/{id}/at/{commit}         → document at specific version
GET    /app/etext/api/documents/{id}/diff?from=X&to=Y    → diff between versions
GET    /app/etext/api/documents/{id}/blame               → blame per section
POST   /app/etext/api/documents/{id}/revert/{commit}     → revert to specific version

# Agent interaction
POST   /app/etext/api/documents/{id}/prompt              → submit prompt to appagent
GET    /app/etext/api/documents/{id}/proposals            → list pending worker proposals

# Real-time
GET    /app/etext/sse/documents/{id}                     → SSE stream for document changes
```

---

## 6. Agent Runtime Contract

### 6.1 Agent Identity and Addressing

Every agent has a globally unique ID (ULID-based, same as cogent's ID generation):

```go
// AgentID format: "agent_" + ULID
// Examples:
//   agent_01HXYZ... (an e-text appagent)
//   agent_01HABC... (a research worker)

func GenerateAgentID() string {
    return "agent_" + ulid.Make().String()
}
```

Agents are addressable by ID. The runtime maintains a registry mapping agent IDs to their location (local goroutine, local process, remote VM + vsock address).

```go
// AgentRegistry tracks all live agents and their locations.
type AgentRegistry struct {
    mu     sync.RWMutex
    agents map[string]AgentLocation
}

type AgentLocation struct {
    AgentID  string
    Kind     string // "local_goroutine", "local_process", "remote_vm"
    Address  string // for remote: "vsock://cid:port" or "http://host:port"
    PID      int    // for local processes
}
```

### 6.2 Messaging (Coagent Communication)

Inter-agent messaging uses **Go channels for in-process agents** and **HTTP API calls for remote agents**. The "one standardized API" is simply a Go interface that also happens to be callable over HTTP — no protocol specs, no SDKs, no compliance surface.

**Task-based delegation** (structured, tracked):
```go
// Appagent delegates to worker
result, err := runtime.DelegateTask(ctx, DelegateRequest{
    From:     appagentID,
    To:       workerID,          // specific worker
    // OR
    ToSkill:  "web_research",    // runtime picks a suitable agent
    Task:     task,
    Budget:   budget,
})
```

**Message-based communication** (lightweight, fire-and-forget or request-response):
```go
// Worker sends proposal to appagent
err := runtime.SendMessage(ctx, Message{
    From:    workerID,
    To:      appagentID,
    Channel: "proposals",
    Parts:   []Part{{Kind: "data", Content: proposalJSON}},
})

// Appagent subscribes to messages
ch := runtime.Subscribe(ctx, appagentID, "proposals")
for msg := range ch {
    // process proposal
}
```

**Channel model** (from cogent's `ChannelManager`, preserved):
Named channels for pub/sub between agents. Used for structured message flows (proposals, status updates, findings). In-process channels are native Go channels. Remote channels serialize messages over HTTP/WebSocket.

### 6.3 Tool Calling

Tools are the agent's hands. They are defined and registered via cogent's existing `ToolRegistry` pattern — plain Go functions registered in a map. The agent runtime dispatches tool calls during the agent's LLM loop.

```go
// ToolRegistry manages available tools for agents.
// Same pattern as cogent's internal/adapters/native/tools.go.
type ToolRegistry struct {
    tools map[string]ToolFunc
}

// ToolFunc is the signature for a tool implementation.
type ToolFunc func(ctx context.Context, params json.RawMessage) (json.RawMessage, error)

// Register adds a tool to the registry.
func (r *ToolRegistry) Register(name string, fn ToolFunc) {
    r.tools[name] = fn
}

// Call invokes a tool by name.
func (r *ToolRegistry) Call(ctx context.Context, name string, params json.RawMessage) (json.RawMessage, error) {
    fn, ok := r.tools[name]
    if !ok {
        return nil, fmt.Errorf("unknown tool: %s", name)
    }
    return fn(ctx, params)
}

// Built-in tools (from cogent's native adapter, preserved):
// - read_file, write_file, edit_file — file I/O
// - glob, grep — file search
// - bash — command execution (sandboxed contexts only)
// - web_search — multi-provider web search (Exa/Tavily/Brave/Serper rotation)
// - web_fetch — URL content fetching
// - git_status, git_diff, git_commit — git operations
// - message_agent — send message to another agent
// - finished — signal task completion
```

**Tool authorization**: The agent runtime filters the tool registry based on the agent's `ToolSet` (§3.1). Workers don't get `write_file` on canonical state; they get `message_agent` instead.

### 6.4 File Access

Agents access files through tool calls. The file access scope depends on execution context:

- **Local agents**: access to the project working directory (scoped by the runtime)
- **VM-sandboxed agents**: access to the VM's filesystem (isolated by the hypervisor)
- **Appagents**: access to their app's data directory plus project files
- **Workers**: access to a temporary working directory; output goes via messages

### 6.5 Communication Architecture

```
In-Process (Go channels)
├── Agent ↔ Agent: direct Go method calls via Agent interface
├── Agent ↔ Tools: direct Go function calls via ToolRegistry
├── Agent ↔ Scheduler: direct Go method calls
├── Agent ↔ EventBus: direct Go channel pub/sub
└── Used for: all local goroutine-based agents (zero overhead)

Remote HTTP (same interface shape, serialized)
├── Host ↔ VM agents: HTTP over vsock
├── Host ↔ external agents: HTTP over TCP
├── Same Go interface serialized as JSON request/response
└── Used for: agents running in microVMs or on remote hosts
```

**In-process** is the primary communication model. When an appagent delegates to a local worker, it's a direct Go function call — typed, zero overhead. When a worker sends a proposal back, it goes through a Go channel.

**Remote** uses the same interface shape serialized as JSON over HTTP. The transport (vsock for VMs, TCP for external) is hidden by the runtime. No protocol specs, no SDKs — just the same Go interfaces callable over HTTP.

### 6.6 Local vs Remote: Same Interface, Transport Hidden

```go
// The caller doesn't know or care where the agent runs.
// The AgentRuntime resolves the agent's location and handles transport.

// This works the same whether workerID is a local goroutine or a remote VM:
result, err := runtime.DelegateTask(ctx, DelegateRequest{
    From: appagentID,
    To:   workerID,
    Task: task,
})
```

**Local dispatch**: Direct Go method call on the agent's `HandleTask`.

**Remote dispatch**: Serialize the `Task` to JSON, send via HTTP POST over vsock to the VM, deserialize the `TaskResult` response. The `AgentRuntime` handles this transparently.

**Implementation**: The `AgentRegistry` (§6.1) maps agent IDs to locations. The runtime creates a transport-appropriate client for each call:

```go
func (r *AgentRuntime) DelegateTask(ctx context.Context, req DelegateRequest) (TaskResult, error) {
    loc, ok := r.registry.Lookup(req.To)
    if !ok {
        return TaskResult{}, fmt.Errorf("agent not found: %s", req.To)
    }

    switch loc.Kind {
    case "local_goroutine":
        agent := r.localAgents[req.To]
        return agent.HandleTask(ctx, req.Task)
    case "local_process":
        return r.ipcClient.SendTask(ctx, loc, req.Task)
    case "remote_vm":
        return r.httpClient.SendTask(ctx, loc.Address, req.Task)
    default:
        return TaskResult{}, fmt.Errorf("unknown agent location kind: %s", loc.Kind)
    }
}
```

### 6.7 Session/Lifecycle Model

Agent sessions follow cogent's existing model (preserved):

```go
// AgentSession tracks an agent's execution context across turns.
type AgentSession struct {
    SessionID     string        `json:"session_id"`
    AgentID       string        `json:"agent_id"`
    ParentSession string        `json:"parent_session,omitempty"` // for worker sessions
    Status        SessionStatus `json:"status"`                   // active, paused, completed, failed
    CreatedAt     time.Time     `json:"created_at"`
    LastTurnAt    time.Time     `json:"last_turn_at"`
}

// Turn represents one agent reasoning step (LLM call + tool executions).
type Turn struct {
    TurnID      string    `json:"turn_id"`
    SessionID   string    `json:"session_id"`
    Input       string    `json:"input"`
    ToolCalls   []ToolCall `json:"tool_calls"`
    Output      string    `json:"output"`
    TokenUsage  Usage     `json:"token_usage"`
    CreatedAt   time.Time `json:"created_at"`
}
```

**Crash recovery**: Cogent's "agents may always stop, the system may always resume" invariant is preserved. Sessions are persisted. The agent runtime can resume a session from the last persisted state.

**History compression**: Cogent's proactive history compression (LLM-based summarization of old turns approaching context limits) is preserved.

---

## 7. Technology Stack

### 7.1 Concrete Recommendations

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| **Language** | Go 1.25+ | Whole Go rewrite |
| **Frontend Framework** | Svelte | Reactive SPA, compiled, small runtime |
| **Text Layout** | Pretext (`@chenglou/pretext`) | DOM-free text measurement for e-text editor |
| **Frontend Embedding** | Go `embed.FS` | Single binary distribution |
| **Agent Comms (local)** | Go channels | Direct, typed, zero overhead |
| **Agent Comms (remote)** | HTTP API + vsock | Same interface shape, serialized |
| **Tool System** | Cogent ToolRegistry | Battle-tested, plain Go functions |
| **LLM Clients** | Cogent native clients | Hand-rolled Anthropic + OpenAI streaming |
| **Supervision** | Custom goroutine supervisor | Lightweight, no framework dependency |
| **Runtime DB** | `modernc.org/sqlite` | Pure Go, already used by cogent |
| **E-Text DB** | Dolt embedded (`dolthub/driver`) | In-process versioned SQL |
| **ID Generation** | `oklog/ulid` | Already used by cogent |
| **Config** | `BurntSushi/toml` | Already used by cogent |
| **CLI** | cobra | Already used by cogent |
| **MicroVM Lifecycle** | `firecracker-go-sdk` | Direct API socket management |
| **VM Images** | Nix (`microvm.nix`) | NixOS guest builds |
| **Auth** | `go-webauthn` | WebAuthn passkey auth |
| **Crypto** | stdlib `crypto/ed25519` | Capability tokens |
| **WebSocket** | `gorilla/websocket` or `coder/websocket` | Real-time frontend comms |
| **HTTP Router** | `net/http` (Go 1.22+) | Standard library |
| **Observability** | OpenTelemetry Go SDK | Structured tracing |

### 7.2 What's NOT in the Stack

- **No A2A protocol SDK** — too heavyweight; plain Go interfaces + HTTP replace it
- **No MCP protocol SDK** — too heavyweight; cogent's ToolRegistry pattern replaces it
- **No ADK-Go** (Google Agent Development Kit) — was only justified by A2A/MCP
- **No actor framework** (Proto.Actor, Ergo, Hollywood) — plain goroutines + channels + custom supervisor
- **No LLM abstraction library** (go-llm, langchaingo, Genkit) — keeping cogent's existing hand-rolled clients
- **No server-side HTML templating** (htmx, templ) — Svelte SPA replaces server-rendered HTML
- **No JavaScript frameworks besides Svelte** (no React, Vue, Alpine.js)
- **No BAML** — Go-native structured output replaces BAML
- **No ractor** — no Rust actor framework
- **No Cargo/Rust toolchain** — pure Go build
- **No separate hypervisor process** — one unified Go binary

---

## 8. Migration Path

### 8.1 What Cogent Code is Preserved/Extended

Cogent is the **foundation** of the unified system. The cogent repo (`/Users/wiz/cogent`) is extended directly — no new repo, no module indirection. Most cogent code is preserved:

| Package | Fate | Notes |
|---------|------|-------|
| `internal/core` | **Preserved** | All types, ID generation, config, capability tokens |
| `internal/store` | **Preserved + extended** | Add desktop state tables, merge hypervisor tables |
| `internal/service` | **Preserved + extended** | Add app lifecycle, desktop session management |
| `internal/cli` | **Preserved + extended** | Add app commands, desktop commands |
| `internal/adapterapi` | **Preserved** | Adapter contract unchanged |
| `internal/adapters/native` | **Preserved + extended** | Core agent loop is the canonical execution engine; ToolRegistry pattern preserved |
| `internal/adapters/claude` | **Preserved** | External adapter support unchanged |
| `internal/events` | **Preserved** | Event translation unchanged |
| `internal/transfer` | **Preserved** | Transfer packets unchanged |
| `internal/debrief` | **Preserved** | Debrief unchanged |
| `internal/catalog` | **Preserved** | Provider catalog unchanged |
| `internal/pricing` | **Preserved** | Pricing registry unchanged |
| `internal/notify` | **Preserved** | Email notifications unchanged |
| `internal/web` | **Extended** | Replace mind-graph-only UI with full web desktop (Svelte SPA) |
| `internal/channelmeta` | **Preserved** | Channel metadata unchanged |
| `client_anthropic.go` | **Preserved + extended** | Hand-rolled Anthropic streaming client, extended for new providers |
| `client_openai.go` | **Preserved + extended** | Hand-rolled OpenAI streaming client, extended for new providers |
| `skills/` | **Preserved** | Skill definitions unchanged |
| `mind-graph/` | **Preserved** | Becomes one app in the desktop, embedded via `embed.FS` |

### 8.2 What Choiros-rs Functionality is Reimplemented in Go

| Choiros-rs Component | Go Implementation | Complexity |
|---------------------|-------------------|------------|
| Provider Gateway (`provider_gateway.rs`, 625 lines) | New `internal/gateway` package. Port the multi-provider routing, Bedrock rewriting, rate limiting, auth injection. | Medium |
| WebAuthn Auth (`auth/`, ~500 lines) | New `internal/auth` package using `go-webauthn/webauthn`. Port registration/login/recovery flows. | Medium |
| Desktop Actor (`desktop.rs`, 2108 lines) | New `internal/desktop` package. Window state management (simpler without actor wrapping — just a service with mutex). | Medium |
| Writer Actor → E-Text AppAgent (`writer/mod.rs`, 2477 lines) | New `internal/apps/etext` package. Reimplemented on Dolt instead of `.qwy` files. Simpler model (no overlays). | High |
| Terminal Actor (`terminal.rs`, 2361 lines) | New `internal/apps/terminal` package. PTY management via Go's `os/exec` + `creack/pty`. Agent harness integration via existing native adapter. | High |
| Conductor → Scheduler (`conductor/`, ~3000 lines) | Merged with cogent's work graph + supervisor. The conductor's capability dispatch becomes scheduler's internal dispatch logic. | High |
| Event Store Actor (`event_store.rs`) | Not needed — direct SQLite access via `internal/store`. | Eliminated |
| Event Bus Actor | Cogent's `EventBus` already exists. Extend it. | Low |
| Supervisor Tree (`supervisor/mod.rs`, 1570 lines) | Replaced by custom goroutine supervisor (§3.7). Simple Go — plain goroutines + channels + restart strategies. | Medium |
| Sandbox Registry (`sandbox/mod.rs`, 1367 lines) | New `internal/vmmanager` package using `firecracker-go-sdk`. Go manages VM lifecycle directly via API socket — no systemd templates, no shell scripts. | High |
| Agent Harness (`agent_harness/`, 3120 lines) | Already exists as cogent's native adapter loop. Extend with OS-layer integration. | Medium |
| Shared Types (`shared-types/src/lib.rs`, 2230 lines) | Distributed across Go packages. No separate types crate needed. | Low |
| Dioxus Frontend (`dioxus-desktop/`, ~6000 lines) | Replaced by Svelte SPA + Pretext for the e-text editor. New `web/` directory in cogent repo for the Svelte build. | High |

### 8.3 Concrete Phasing

#### Phase 1: Foundation (Weeks 1-4)

**Goal**: Extend the cogent repo into a unified Go process that can boot, authenticate, serve a web desktop, and run the existing cogent work graph.

1. Extend cogent's `serve.go` to serve a Svelte-based web desktop (JSON API + embedded static assets) instead of just the mind-graph
2. Add WebAuthn authentication (port from choiros hypervisor)
3. Add desktop state management (windows, sessions) to the store
4. Serve a basic desktop shell with apps: mind-graph, settings, logs
5. Provider gateway as a Go package (port from choiros `provider_gateway.rs`)
6. All existing cogent CLI commands continue to work

**Deliverable**: A single `cogent serve` that shows a Svelte web desktop with auth. Existing work graph and adapter system fully functional.

#### Phase 2: Agent Runtime Unification (Weeks 5-8)

**Goal**: One agent runtime with local and remote execution, standardized contract.

1. Define and implement the `Agent` interface (§3.1)
2. Implement the custom goroutine supervisor (§3.7) for agent lifecycle management
3. Implement `AgentRegistry` with local goroutine and local process backends
4. Extend native adapter to implement the `Agent` interface
5. Implement HTTP-based remote agent communication (same interface, serialized over HTTP/vsock)
6. Extend cogent's ToolRegistry with additional tools ported from choiros agent harness
7. Implement agent-to-agent messaging (channels, proposals)

**Deliverable**: Appagents can delegate tasks to workers. Workers can send messages back. Same API for local and remote.

#### Phase 3: E-Text App (Weeks 9-12)

**Goal**: The e-text app as a fully functional reference implementation.

1. Integrate Dolt embedded driver
2. Implement e-text Dolt schema and `ETextStore`
3. Build e-text AppAgent (handles prompts, delegates to workers, commits to Dolt)
4. Build e-text Svelte UI with Pretext for high-performance text measurement and layout
5. Implement version history, diff, blame via Dolt system tables
6. Implement worker proposal flow (message → appagent → commit)
7. JSON API surface for all e-text operations

**Deliverable**: Users can create documents, edit them directly in the Svelte/Pretext editor, prompt the agent, see version history. Workers produce proposals that the appagent applies.

#### Phase 4: VM Integration (Weeks 13-16)

**Goal**: MicroVM sandboxing for untrusted code execution.

1. Implement `internal/vmmanager` using `firecracker-go-sdk` — Go manages VM lifecycle directly via API socket calls
2. Port VM lifecycle (boot, stop, hibernate, idle watchdog, memory pressure) — no systemd templates, no shell scripts
3. Implement vsock-based HTTP transport for host ↔ guest communication
4. Implement terminal app with PTY management inside VMs
5. Capability token enforcement at the VM boundary
6. Agent processes inside VMs implement the `Agent` interface via HTTP over vsock

**Deliverable**: Code execution happens in sandboxed VMs. Same agent API works for local and VM agents.

#### Phase 5: Polish and Migration (Weeks 17-20)

**Goal**: Feature parity, migration tooling, production readiness.

1. Machine class support (different VM profiles)
2. Branch/dev sandboxes
3. Admin API (sandbox management, system stats)
4. Migration tooling for existing choiros-rs users (export events.db data, import documents to Dolt)
5. NixOS deployment configuration
6. E2E testing
7. Performance optimization

**Deliverable**: Production-ready unified system.

### 8.4 What Can Be Dropped Entirely

| Component | Why It Can Be Dropped |
|-----------|----------------------|
| `shared-types` crate | Single-language system — no need for cross-language type sharing |
| BAML code generation | Go-native structured output replaces BAML |
| ractor dependency | Replaced by plain goroutines + channels + custom supervisor |
| `.qwy` file format | Replaced by Dolt relational storage |
| Overlay/pending system | v1 uses serialized writes; no overlay/branch isolation needed |
| Event Store Actor | Direct SQLite access — no actor wrapper needed |
| Gateway token dance (kernel cmdline injection) | Simplified — vsock channel or direct in-process access |
| Separate hypervisor process | Unified into one Go process |
| Separate sandbox process | Unified — the Go process IS the sandbox runtime |
| `dioxus-desktop` WASM frontend | Replaced by Svelte SPA + Pretext |
| `ts-rs` TypeScript generation | Not applicable to the new system |
| `SQLX_OFFLINE` / `.sqlx/` directory | Not applicable to Go |
| `baml_src/` definitions | Replaced by Go-native LLM function contracts |
| Cargo workspace, Cargo.lock | Go modules replace Cargo |
| systemd template layer | Go manages VMs directly via firecracker-go-sdk |
| vfkit-runtime-ctl scripts | Go manages VMs directly via firecracker-go-sdk |

---

## 9. Mapping Table

| Current Component | Source | Fate in Unified System | Notes |
|---|---|---|---|
| **Hypervisor HTTP server** | choiros-rs | **Merged** into unified Go HTTP server | Auth routes, admin routes, proxy routes all served by one process |
| **Sandbox HTTP server** | choiros-rs | **Merged** into unified Go HTTP server | Desktop, app, terminal, file routes all served by one process |
| **Provider Gateway** | choiros-rs | **Reimplemented** as `internal/gateway` Go package | Same logic: multi-provider routing, key injection, rate limiting |
| **WebAuthn auth** | choiros-rs | **Reimplemented** using `go-webauthn/webauthn` | Same flow: registration, login, recovery, session cookies |
| **SandboxRegistry** | choiros-rs | **Reimplemented** as `internal/vmmanager` | Uses `firecracker-go-sdk` direct API socket calls — no systemd/shell scripts |
| **Machine Classes** | choiros-rs | **Preserved** concept, reimplemented in Go | TOML config, user preference, admin override |
| **Route Pointers** | choiros-rs | **Simplified** — single process, no routing needed | Eliminated: no hypervisor↔sandbox boundary |
| **Session Store** | choiros-rs | **Merged** into runtime DB | SQLite sessions table |
| **EventStoreActor** | choiros-rs | **Eliminated** | Direct SQLite writes via `internal/store` |
| **EventBusActor** | choiros-rs | **Merged** with cogent's `EventBus` | In-process pub/sub, same pattern |
| **EventRelayActor** | choiros-rs | **Eliminated** | No need — EventBus is directly connected to persistence |
| **ConductorActor** | choiros-rs | **Merged** into Scheduler | Conductor's dispatch logic becomes scheduler's internal dispatch |
| **WriterActor** | choiros-rs | **Reimplemented** as E-Text AppAgent | Dolt-backed instead of .qwy files; simpler model |
| **TerminalActor** | choiros-rs | **Reimplemented** as Terminal AppAgent | Go PTY management via `creack/pty` |
| **ResearcherActor** | choiros-rs | **Subsumed** by worker agents | Research is a worker task, not a separate actor type |
| **DesktopActor** | choiros-rs | **Reimplemented** as `internal/desktop` | Window state management, simpler Go service |
| **MemoryActor** | choiros-rs | **Reimplemented** as `internal/memory` | Per-user symbolic memory, SQLite-backed |
| **AgentHarness** | choiros-rs | **Merged** with cogent native adapter loop | Cogent's loop is the canonical implementation |
| **ALM Harness** | choiros-rs | **Deferred** | Complex execution DAGs are a future enhancement |
| **ApplicationSupervisor** | choiros-rs | **Replaced** by custom goroutine supervisor (§3.7) | Plain goroutines + channels + restart strategies |
| **WriterDelegationAdapter** | choiros-rs | **Replaced** by Go channels + HTTP task delegation | Standard in-process channels for local, HTTP for remote |
| **BAML contracts** | choiros-rs | **Replaced** by Go-native structured output | Cogent's native LLM clients with JSON schema support |
| **shared-types** | choiros-rs | **Eliminated** | Go types in relevant packages |
| **Dioxus WASM frontend** | choiros-rs | **Replaced** by Svelte SPA with Pretext | Client-side rendered, no WASM |
| **Nix build (Crane)** | choiros-rs | **Replaced** by Nix Go builder | Standard Nix Go build derivation |
| **`cogent.db` schema (21 tables)** | cogent | **Preserved** | Foundation of runtime DB |
| **`cogent-private.db`** | cogent | **Preserved** | Gitignored private data |
| **Work graph state machine** | cogent | **Preserved** as scheduler internals | Same state machine, internal-only visibility |
| **Attestation model** | cogent | **Preserved** | Quality gate for work completion |
| **Adapter system** | cogent | **Preserved** | `Adapter` + `LiveAgentAdapter` interfaces unchanged |
| **Native adapter** | cogent | **Preserved + extended** | Core agent loop, co-agents, channels, history compression, ToolRegistry |
| **Claude adapter** | cogent | **Preserved** | External adapter unchanged |
| **EventBus** | cogent | **Extended** to serve app-layer events | Add work→app event bridging |
| **WebSocket hub** | cogent | **Extended** | Serve desktop + app events |
| **Serve runtime** | cogent | **Extended** to be the unified JSON API server | JSON API + WS + SSE + embedded Svelte static assets |
| **CLI commands** | cogent | **Preserved + extended** | Add app management commands |
| **Capability tokens** | cogent | **Preserved** | Ed25519 CA, agent auth |
| **Rotation config** | cogent | **Preserved** | Adapter/model rotation for dispatch |
| **Briefing/hydration** | cogent | **Preserved** | ProjectHydrate for supervisor context |
| **Email notifications** | cogent | **Preserved** | Digest emails via Resend |
| **Mind-graph UI** | cogent | **Preserved** as desktop app | Embedded in web desktop via `embed.FS` as one app |
| **Co-agent tools** | cogent | **Extended** to use Go channels (local) and HTTP (remote) | coagent_spawn/send/status via Agent interface |
| **Channel manager** | cogent | **Preserved** | Inter-agent message channels |
| **History compression** | cogent | **Preserved** | LLM-based context compression |
| **Session persistence** | cogent | **Preserved** | Native sessions persisted to filesystem |
| **Catalog/pricing** | cogent | **Preserved** | Provider discovery and cost tracking |
| **LLM clients** | cogent | **Preserved + extended** | Hand-rolled Anthropic + OpenAI streaming clients; extend for new providers |

---

## 10. Open Questions

### 10.1 Architecture

1. **System name** — still TBD. Referred to as "the system" throughout this document.

2. **Dolt binary size impact**: The embedded Dolt driver pulls in the entire Dolt engine, potentially adding 100MB+ to binary size. Is this acceptable? Alternative: use Dolt as a sidecar process with MySQL protocol. Decision needed before Phase 3.

3. **Single process vs. multi-process on host**: The spec describes a single unified Go process. For very large deployments, should there be an option to split into control plane + data plane processes? Or is the single process sufficient given that heavy work happens in VMs?

4. **Pretext integration depth** — just for the e-text editor, or used more broadly for text measurement across all apps in the desktop? Pretext is purpose-built for high-performance text layout without DOM reflow, which could benefit other text-heavy UI components beyond e-text.

### 10.2 Migration

5. **Existing user data migration**: Users on the current choiros-rs system have data in `events.db` and `.qwy` files. What's the migration path? Export + import tooling needed.

6. **Parallel operation period**: Can the old (choiros-rs) and new (Go) systems run side-by-side during migration? Or is it a hard cutover?

7. **cogent CLI backward compatibility**: The `cogent` CLI is used by external agents (Claude Code, etc.) via subprocess. Must all existing CLI commands remain backward-compatible?

### 10.3 Design

8. **App isolation model**: How strongly are apps isolated from each other? Can the e-text appagent access the terminal app's resources? The current design says "no" but this needs enforcement details.

9. **Multi-user**: The current system is single-user per sandbox. The unified system should support multiple users on one host with per-user VMs. How does the desktop shell handle multiple simultaneous users? (Likely: separate sessions, no shared state.)

10. **Appagent model selection**: Each appagent needs to make LLM calls. Should appagents use a single model (chosen at config time) or have model rotation like the scheduler? The e-text appagent probably needs a strong model (Claude Opus) while research workers can use cheaper models.

11. **Worker lifecycle**: Are workers persistent (long-running sessions) or ephemeral (spawn per task, die after)? Cogent's native adapter supports persistent sessions. The unified system should probably support both.

---

## 11. Risk Register

### 11.1 Critical Risks

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|------------|
| R1 | **Hidden orchestration complexity** (design constraint #8) — the unified system becomes as complex as the two separate systems combined, defeating the purpose | High | Critical | Ruthless simplification. The scheduler is internal. No user-facing work items. Apps handle their own UX. Resist the temptation to add orchestration layers. |
| R2 | **Dolt embedded driver instability** — the embedded Go driver is less battle-tested than Dolt server mode | Medium | High | Mitigate with extensive integration testing. Have a fallback plan to switch to Dolt server mode (MySQL protocol) if embedded mode has issues. Keep the `database/sql` interface so the switch is easy. |
| R3 | **Binary size explosion** — Dolt engine + Svelte build + all Go dependencies + embedded assets → binary exceeds 200MB | Medium | Medium | Monitor binary size. If excessive: (a) strip debug symbols, (b) consider Dolt as sidecar, (c) lazy-load Dolt only when e-text app is used, (d) optimize Svelte build with tree-shaking. |
| R4 | **Feature regression during rewrite** — the Go system doesn't reach feature parity with choiros-rs before the old system degrades | High | High | Phased migration (§8.3). Each phase delivers standalone value. Don't deprecate choiros-rs until Phase 4 is complete. |

### 11.2 Significant Risks

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|------------|
| R5 | **Writer/E-Text complexity underestimated** — the choiros WriterActor is 2477 lines of complex state management (versions, overlays, patches, delegation). Reimplementing on Dolt may not be simpler. | Medium | Medium | The Dolt rewrite should be *simpler* because Dolt handles versioning natively (no custom version tree). But the appagent delegation logic is the real complexity. Start with a minimal e-text appagent and iterate. |
| R6 | **Svelte + Pretext integration risk** — the e-text editor is a complex UI component combining Svelte's reactive model with Pretext's DOM-free text measurement. This is a novel integration that may have unforeseen challenges. | Medium | Medium | Prototype the Svelte + Pretext editor early (Phase 3). Start with a minimal text area and incrementally add Pretext-powered features. Have a fallback to a simpler editor if Pretext integration proves too difficult. |
| R7 | **Single-binary size risk** — Dolt engine + embedded Svelte build + static assets could make the binary large enough to impact deployment and startup time. | Medium | Medium | Measure early. Svelte builds are typically small (~50KB gzipped). Dolt is the main concern. Consider Dolt as optional sidecar if size exceeds targets. |
| R8 | **Cogent codebase modification risk** — extending cogent in-place risks breaking its existing functionality | Low | High | Comprehensive test suite (cogent has E2E tests). Feature flags for new functionality. CI/CD gates on all existing tests. |
| R9 | **Vsock complexity on macOS dev** — vsock is Linux-only; macOS development requires a different IPC mechanism | Medium | Low | Use TCP as the dev transport; vsock as the production transport. The HTTP interface is transport-agnostic. |

### 11.3 Low Risks (Monitor)

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|------------|
| R10 | **Go 1.25+ dependency** — requires recent Go version | Low | Low | NixOS pins the Go version. Not a concern for deployment. |
| R11 | **WebAuthn library differences** — `go-webauthn` may have behavioral differences from `webauthn-rs` | Low | Low | Test with existing credentials. Worst case: users re-register. |
| R12 | **Dolt MySQL compatibility gaps** — edge cases in SQL syntax | Low | Low | E-text uses simple CRUD — well within Dolt's compatibility. |

---

*End of spec sketch. This document should be reviewed and iterated before implementation begins.*
