# Cogent Architecture — Comprehensive Analysis

**Generated:** 2026-04-08
**Source:** /Users/wiz/cogent (Go codebase)
**Purpose:** Ground-truth architectural reference for choiros-rs↔cogent unification

---

## 1. System Overview

**Cogent** is a standalone Go CLI that acts as a **local control plane for governed agent work**. It is NOT itself a coding agent — it's an orchestration layer that:

- Manages a **work graph** (SQLite) tracking work items, dependencies, attestations, approvals, and promotions
- Provides a **canonical lifecycle** state machine for work items: `ready → claimed → in_progress → blocked → done / failed / cancelled / archived`
- Runs **adapter-backed agent sessions** via subprocess invocation (Claude, native Go adapter) or live API sessions
- Offers a **serve runtime** (`cogent serve --auto`) that starts a web UI, HTTP API, WebSocket event bus, and an agentic supervisor that auto-dispatches work
- Implements **cryptographic agent identity** (Ed25519 CA → per-agent capability tokens)
- Enforces an **attestation-gated verification model**: work is done only when durable evidence (tests, reviews, checks) satisfies the attestation policy

**Core invariant:** "Agents may always stop, the system may always resume."

**Key dependencies:** Go 1.25+, `cobra` (CLI), `modernc.org/sqlite` (pure-Go SQLite), `oklog/ulid` (IDs), `BurntSushi/toml` (config).

---

## 2. Package Map

### `internal/core` — Domain types and configuration
- **Purpose:** Canonical types, ID generation, path resolution, config loading, and cryptographic capability system
- **Key types:**
  - `JobState` — job lifecycle: created/queued/starting/running/waiting_input/completed/failed/cancelled/blocked
  - `WorkExecutionState` — work lifecycle: ready/claimed/in_progress/checking/blocked/done/failed/cancelled/archived
  - `WorkApprovalState` — none/pending/verified/rejected
  - `WorkLockState` — unlocked/human_locked
  - `JobRecord`, `SessionRecord`, `TurnRecord`, `EventRecord`, `ArtifactRecord` — canonical persistence records
  - `WorkItemRecord` — the central work entity with rich metadata (preferred adapters, model traits, required attestations, required docs, acceptance criteria, attempt epoch)
  - `WorkEdgeRecord` — graph edges between work items (blocks, parent-child, etc.)
  - `AttestationRecord` — signed verification evidence with method, verifier kind, confidence, blocking flag
  - `ApprovalRecord`, `PromotionRecord` — approval and environment promotion
  - `CheckRecord`, `CheckReport` — structured checker output (build status, test counts, screenshots, videos)
  - `DocContentRecord` — docs-as-verification: in-DB doc content with version tracking and repo-file-exists checks
  - `WorkProposalRecord` — proposals for graph mutations (add edge, merge, split)
  - `TransferPacket`, `TransferRecord` — cross-adapter context transfer
  - `CatalogEntry`, `CatalogSnapshot` — provider/model discovery with auth method, billing class, pricing
  - `UsageReport`, `CostEstimate`, `UsageAttribution` — token usage and cost tracking
  - `CapabilityToken`, `CAKeyPair`, `AgentCredential` — Ed25519-signed capability tokens
  - `Config`, `Paths` — TOML config and XDG-style path resolution
  - `RotationConfig`, `RotationEntry` — configurable model rotation pool
- **Dependencies:** `oklog/ulid`, `BurntSushi/toml`, stdlib crypto/ed25519
- **Public API:** All types exported; `GenerateID(prefix)`, `LoadConfig(path)`, `ResolvePathsForRepo()`, `EnsureCA(stateDir)`, `IssueToken(...)`, `VerifyToken(...)`, `SignJSON(...)`, capability role mappings

### `internal/store` — SQLite persistence
- **Purpose:** All database operations — CRUD for every entity, WAL management, corruption recovery
- **Key types:** `Store` struct wrapping `*sql.DB` (public) + `*sql.DB` (private/gitignored)
- **Dependencies:** `internal/core`, `modernc.org/sqlite`
- **Public API:** `Open(ctx, path)`, `OpenWithPrivate(ctx, pub, priv)`, `Close()`, `CheckpointWAL()`, plus ~80 CRUD methods for sessions, jobs, turns, events, artifacts, locks, work items, edges, updates, notes, proposals, attestations, approvals, promotions, doc content, check records, catalog snapshots, job runtime, private notes, history search
- **Notable:** Auto-recovery from corruption (sqlite3 `.recover`), `_txlock=immediate`, WAL mode, `MaxOpenConns=1`

### `internal/service` — Business logic layer
- **Purpose:** Orchestrates store operations, adapter lifecycle, event publishing, work graph logic, briefing/hydration, usage accounting, verification, notifications
- **Key types:**
  - `Service` — central service struct holding Paths, Config, store, EventBus, DigestCollector
  - `RunRequest`, `SendRequest`, `DebriefRequest` — job launch parameters
  - `RunResult` — job + session IDs returned from launches
  - `EventBus` — in-process pub/sub for `WorkEvent` with subscribe/unsubscribe/publish
  - `WorkEvent` — typed events with `Kind`, `WorkID`, `Actor`, `Cause`, `RequiresSupervisorAttention()` gate
- **Sub-files by concern:**
  - `service.go` — core job launch/status/logs/cancel/send/debrief/list/session/history/artifacts/catalog
  - `service_work.go` — work CRUD, claim/release, lease renewal, edge management, proposals, state transitions
  - `service_attestation.go` — attestation creation, completion gating, doc-as-verification checks
  - `service_briefing.go` — `ProjectHydrate()` — deterministic briefing compilation for supervisor and worker modes
  - `service_docs.go` — doc-set, doc sync, required-docs normalization
  - `service_graph.go` — graph traversal, children, blocking edge logic
  - `service_job.go` — detailed job lifecycle management (process spawn, output capture, event translation)
  - `service_notify.go` — email notifications via Resend API (fire-and-forget)
  - `service_proof.go` — proof/evidence collection for attestations
  - `service_state.go` — state transition validation and enforcement
  - `service_supervisor.go` — supervisor-specific logic (ready work detection, dispatch decisions)
  - `service_usage.go` — token usage aggregation, cost estimation, usage attribution
  - `events.go` — `EventBus`, `WorkEvent`, actor/cause classification, `RequiresSupervisorAttention()` filter
  - `bootstrap.go` — filesystem inspection for work graph bootstrapping from code/docs
  - `verify.go` — verification pipeline: check → attestation → completion
- **Dependencies:** All other `internal/` packages
- **Public API:** `Open(ctx, configPath)`, `Close()`, `Run(ctx, req)`, `Send(ctx, req)`, `Debrief(ctx, req)`, `Status(ctx, jobID)`, `Logs(ctx, jobID)`, `Cancel(ctx, jobID)`, `List*(...)`, `CreateWork(...)`, `UpdateWork(...)`, `ClaimWork(...)`, `AttestWork(...)`, `ApproveWork(...)`, `PromoteWork(...)`, `ProjectHydrate(...)`, `CatalogSync(...)`, `CatalogProbe(...)`, `HistorySearch(...)`, `BootstrapCreate(...)`, etc.

### `internal/cli` — CLI commands and serve runtime
- **Purpose:** Cobra command tree, HTTP API server, WebSocket hub, agentic supervisor, housekeeping
- **Key files:**
  - `root.go` (~3800 lines) — ALL cobra commands defined here: run, status, logs, send, debrief, cancel, list, session, artifacts, history, catalog, runtime, adapters, transfer, work (create/list/show/ready/update/complete/note-add/private-note/doc-set/claim/release/renew-lease/children/discover/proposal/attest/approve/reject/promote/check/hydrate/force-done/bootstrap/log), serve, login, version, inbox, capacity
  - `serve.go` (~2900 lines) — HTTP server, WebSocket hub, all API route handlers, housekeeping goroutines (stall detection, WAL checkpointing, digest collection), supervisor lifecycle
  - `supervisor.go` (~230 lines) — round-robin adapter/model rotation for dispatch, `pickAdapterModel()` logic
  - `supervisor_agent.go` (~470 lines) — agentic supervisor as a live adapter session (ADR-0041): hydration → dispatch → monitor → react loop
  - `dispatch.go` — `dispatchWork()` function bridging CLI to service for work dispatch
  - `client.go` — HTTP client for CLI→serve delegation
  - `login.go` — login/auth commands
  - `exit.go` — `ExitError` type with exit codes
  - `check_and_report.go` — checker dispatch and reporting
  - `capability.go` — capability token CLI commands
  - `project.go` — project-level commands
- **Dependencies:** `internal/service`, `internal/adapters`, `internal/core`, `internal/web`, `cobra`
- **Notable:** The serve runtime is a single Go process handling: HTTP API, embedded web UI (via `internal/web`), WebSocket real-time events, housekeeping timers, and an optional agentic supervisor session.

### `internal/adapterapi` — Adapter contract
- **Purpose:** Defines the interface contract for all adapters
- **Key types:**
  - `Adapter` interface — `Name()`, `Capabilities()`, `Implemented()`, `Binary()`, `Detect(ctx)`, `StartRun(ctx, req)`, `ContinueRun(ctx, req)`
  - `LiveAgentAdapter` interface — `StartSession(ctx, req)`, `ResumeSession(ctx, nativeID, req)` for persistent live sessions
  - `LiveSession` interface — `SessionID()`, `ActiveTurnID()`, `StartTurn(ctx, input)`, `Steer(ctx, turnID, input)`, `Interrupt(ctx)`, `Events()`, `Close()`
  - `Capabilities` — flags: HeadlessRun, StreamJSON, NativeResume, NativeFork, StructuredOutput, InteractiveMode, RPCMode, MCP, Checkpointing, SessionExport
  - `Diagnosis` — adapter detection result
  - `StartRunRequest`, `ContinueRunRequest`, `RunHandle` — subprocess launch parameters and handle
  - `BaseAdapter` — generic subprocess-based adapter implementation via `Builder` interface
  - `Event`, `EventKind` — live session events: session.started/resumed/closed, turn.started/completed/failed/interrupted, output.delta, error
  - `SteerEvent` — mid-turn steering message
- **Dependencies:** stdlib only
- **Public API:** All types and interfaces exported

### `internal/adapters` — Adapter registry and implementations
- **Purpose:** Registry that resolves adapter name → implementation; contains all adapter implementations
- **Key files:**
  - `registry.go` — `Resolve(ctx, cfg, name)` and `CatalogFromConfig(cfg)` mapping to claude and native adapters
  - `claude/adapter.go` — Claude Code CLI adapter (subprocess-based via BaseAdapter)
  - `native/` — **Native Go adapter** (the most complex adapter, ~26 files):
    - `adapter.go` — adapter lifecycle, process spawning, history injection
    - `session.go` — `nativeSession` struct managing turn lifecycle, tool execution, history
    - `loop.go` — LLM tool-calling loop: call → parse response → execute tools → repeat until end_turn
    - `client.go` — `LLMClient` interface abstracting Anthropic Messages API vs OpenAI Responses API
    - `client_anthropic.go` — Anthropic streaming client implementation
    - `client_openai.go` — OpenAI streaming client implementation
    - `provider.go` — provider configuration (Anthropic, OpenAI, ZAI, Bedrock, ChatGPT)
    - `auth.go` — provider-specific auth (API keys, bearer tokens, ChatGPT subscription auth)
    - `tools.go` — `Tool`, `ToolRegistry`, `ToolFunc` — tool definition and dispatch framework
    - `tools_coding.go` — coding tools: read_file, write_file, edit_file, glob, grep, bash, git_status, git_diff, git_commit
    - `tools_web.go` — web tools: web_search (multi-provider rotation: Exa/Tavily/Brave/Serper), web_fetch
    - `tools_cogent.go` — cogent tools: check_record_create/list/show, run_tests, run_playwright
    - `tools_coagent.go` — co-agent tools: coagent_spawn, coagent_send, coagent_status, coagent_wait, coagent_interrupt, coagent_list, channel_send, channel_read, channel_subscribe — for multi-agent orchestration
    - `channel.go` — `ChannelManager` for inter-agent message passing
    - `history_compress.go` — proactive history compression when approaching context window limits
    - `session_persist.go` — session state persistence to `.cogent/native-sessions/<id>.json`
- **Dependencies:** `internal/adapterapi`, `internal/core`
- **Notable:** The native adapter is a full-featured coding agent runtime — it talks directly to LLM APIs (Anthropic, OpenAI), manages tool calling loops, supports multi-agent orchestration via co-agents and channels, and persists session history across process restarts.

### `internal/events` — Event translation
- **Purpose:** Translates raw adapter stdout/stderr JSON lines into canonical event hints
- **Key function:** `TranslateLine(adapter, stream, line) []Hint` — parses JSON lines and extracts session.discovered, assistant.delta, assistant.message, tool.call, tool.result, usage, diagnostic hints
- **Dependencies:** stdlib only

### `internal/transfer` — Transfer packet rendering
- **Purpose:** Renders `TransferPacket` into a human/agent-readable prompt for cross-adapter failover
- **Key function:** `RenderPrompt(targetAdapter, packet) string`
- **Dependencies:** `internal/core`

### `internal/debrief` — Debrief prompt rendering
- **Purpose:** Renders a "land the plane" prompt for model-authored session summaries
- **Key function:** `RenderPrompt(session, adapter, reason) string`
- **Dependencies:** `internal/core`

### `internal/catalog` — Provider/model discovery
- **Purpose:** Discovers available providers, models, auth modes from installed adapters
- **Key function:** `Snapshot(ctx, cfg, runner) CatalogSnapshot` — probes claude and native adapters for available models
- **Dependencies:** `internal/adapters`, `internal/core`

### `internal/pricing` — Model pricing registry
- **Purpose:** Built-in pricing for known models (OpenAI, Anthropic, Google) plus config overrides
- **Key function:** `Resolve(cfg, provider, model) *ModelPricing`, `Estimate(usage, pricing) *CostEstimate`
- **Dependencies:** `internal/core`

### `internal/notify` — Email notifications
- **Purpose:** Fire-and-forget email via Resend API
- **Key files:**
  - `email.go` — `SendEmail(ctx, apiKey, to, subject, html, attachments)`
  - `email_builder.go` — HTML email template builder for digests
  - `digest.go` — `DigestCollector` that aggregates work events into periodic email digests
- **Dependencies:** `internal/core`, stdlib net/http

### `internal/web` — Embedded web UI
- **Purpose:** `embed.FS` wrapping `dist/` static assets (mind-graph UI, index.html)
- **Dependencies:** stdlib embed

### `internal/channelmeta` — Channel metadata helpers
- **Purpose:** Normalize worker report types and metadata tags for channel messages
- **Dependencies:** stdlib only

---

## 3. Work Graph

### Work Item Creation & Tracking
- Created via `cogent work create --title "..." --objective "..." --kind implement`
- Each work item gets a ULID-based `work_id` with prefix `work_`
- Items have rich metadata: priority, position, configuration_class, budget_class, required_capabilities, required_model_traits, preferred/forbidden adapters and models, acceptance criteria
- Docs bootstrapping: `cogent work doc-set --file docs/adr-001.md` auto-creates or attaches docs to work items

### State Machine
```
ready → claimed → in_progress → done
                              → failed
                              → cancelled
                              → blocked → (back to in_progress or ready)
                              → archived
```
- `checking` and `awaiting_attestation` are deprecated aliases that canonicalize to `in_progress`
- `WorkExecutionState.Canonical()` normalizes on read
- `WorkExecutionState.Terminal()` returns true for done/failed/cancelled/archived
- Transitions validated in `service_state.go`

### Approval Flow
- `none → pending → verified / rejected`
- Approval requires attestation evidence
- Approvals are immutable records with optional `supersedes_approval_id`

### Graph Edges
- `WorkEdgeRecord` connects work items with typed edges (`blocks`, `parent_child`, etc.)
- Edges are directional: `from_work_id` → `to_work_id`
- Blocking edges gate execution: blocked items can't complete until blockers are done
- Graph traversal via `service_graph.go`

### Position/Ordering
- `priority` (integer) and `position` (integer) fields on work items
- Used by supervisor for dispatch ordering

### Attempt Tracking
- `attempt_epoch` starts at 1, increments on retry/reset
- Stale children/nonces/review artifacts from prior attempts don't satisfy new runs

### Lease Model
- `claimed_by` + `claimed_until` for time-limited claims
- `cogent work claim`, `release`, `renew-lease` commands
- Stale leases detected by housekeeping

---

## 4. Scheduler / Dispatch

### Serve Runtime (`cogent serve`)
The serve command starts a single Go process with:
1. **HTTP API server** (default port 4242) — RESTful JSON API for all work/job/session operations
2. **Embedded Web UI** — mind-graph visualization served from embedded assets
3. **WebSocket hub** — real-time event broadcasting to connected clients
4. **Housekeeping goroutines:**
   - Stall detection (identifies stuck jobs)
   - WAL checkpointing (periodic SQLite maintenance)
   - Digest collection (aggregates events for email notifications)
5. **Agentic supervisor** (when `--auto` flag) — an LLM-powered session that monitors the EventBus and dispatches work

### Auto-Dispatch Flow
1. Supervisor session starts with a `ProjectHydrate(mode="supervisor")` briefing
2. Supervisor subscribes to `EventBus` for `WorkEvent` notifications
3. Events filtered through `RequiresSupervisorAttention()` — excludes supervisor's own mutations, housekeeping noise, mid-run progress
4. On wake: supervisor re-hydrates context, examines ready work, dispatches via its tool-calling loop
5. Dispatch uses `pickAdapterModel()` which applies:
   - Work item's preferred adapters/models
   - Round-robin rotation across configured pool
   - Job history to avoid repeating last adapter

### Rotation Configuration
```toml
[[rotation.entries]]
adapter = "native"
model = "chatgpt/gpt-5.4-mini"
max_runs_per_day = 0
roles = []
```
Default rotation: native/chatgpt, native/zai, native/bedrock, claude/sonnet, claude/haiku

### Manual Claim
- `cogent work claim <work-id> --claimant worker-a`
- `cogent work release <work-id> --claimant worker-a`
- `cogent work renew-lease <work-id> --claimant worker-a --lease 15m`

### Job Launch
1. Validate flags → load config → resolve adapter → `Detect()`
2. Create session + job records in SQLite
3. Spawn vendor process (subprocess or live session)
4. Persist `process.spawned` event
5. Stream raw stdout/stderr into artifact files
6. Translate canonical events as lines arrive
7. Finalize job state on completion

---

## 5. Adapter System

### Current Adapters
| Adapter | Type | Description |
|---------|------|-------------|
| `claude` | Subprocess (BaseAdapter) | Claude Code CLI headless mode (`claude -p --output-format stream-json`) |
| `native` | Live API session | Full Go-native coding agent with direct Anthropic/OpenAI API calls |

### Adapter Contract (`adapterapi.Adapter`)
```go
type Adapter interface {
    Name() string
    Capabilities() Capabilities
    Implemented() bool
    Binary() string
    Detect(ctx context.Context) (Diagnosis, error)
    StartRun(ctx context.Context, req StartRunRequest) (*RunHandle, error)
    ContinueRun(ctx context.Context, req ContinueRunRequest) (*RunHandle, error)
}
```

### Live Agent Contract (`adapterapi.LiveAgentAdapter`)
```go
type LiveAgentAdapter interface {
    Name() string
    StartSession(ctx context.Context, req StartSessionRequest) (LiveSession, error)
    ResumeSession(ctx context.Context, nativeSessionID string, req StartSessionRequest) (LiveSession, error)
}
```

### Native Adapter Deep-Dive
The native adapter is a full coding agent runtime:
- **LLM clients:** Anthropic Messages API (streaming) and OpenAI Responses API (streaming)
- **Provider routing:** ZAI (GLM-5-turbo), Bedrock (Claude Haiku), ChatGPT (GPT-5.4-mini), direct Anthropic, direct OpenAI
- **Tool system:** Extensible `ToolRegistry` with coding tools (read/write/edit/glob/grep/bash/git), web tools (search with Exa/Tavily/Brave/Serper rotation, fetch), cogent tools (check records, test runner, Playwright), co-agent tools (spawn/send/status/wait/interrupt sub-agents)
- **Multi-agent:** `coAgentManager` can spawn and manage child agent sessions, with inter-agent `ChannelManager` for message passing
- **History:** Session history persisted to `.cogent/native-sessions/<id>.json`, proactive compression when approaching context limits
- **Extended thinking:** Supports Anthropic thinking blocks with signature preservation for multi-turn

### Configuration
```toml
[adapters.claude]
binary = "claude"
enabled = true

[adapters.native]
binary = "cogent"
enabled = true
```

---

## 6. CLI Surface

### Core Job Commands
| Command | Description |
|---------|-------------|
| `cogent run` | Start a new job (--adapter, --cwd, --prompt) |
| `cogent status` | Job state + usage + cost (--wait, --json) |
| `cogent logs` | Stream canonical events or raw output (--follow, --raw) |
| `cogent send` | Continue a native session with new input |
| `cogent debrief` | Model-authored session summary |
| `cogent cancel` | Cancel running job (SIGINT → SIGTERM → SIGKILL) |
| `cogent list` | List jobs or sessions with filters |
| `cogent session` | Show canonical session state |
| `cogent artifacts list/show` | Inspect persisted artifacts |
| `cogent history search` | Search local history across jobs/turns/events/artifacts |

### Work Graph Commands
| Command | Description |
|---------|-------------|
| `cogent work create` | Create work item (--title, --objective, --kind) |
| `cogent work list` | List work items with filters |
| `cogent work show` | Full work item details + docs + attestations + notes |
| `cogent work ready` | List actionable work items |
| `cogent work update` | Update execution/approval state |
| `cogent work complete` | Mark work done (with completion gating) |
| `cogent work claim/release/renew-lease` | Lease management |
| `cogent work note-add` | Add notes to work items |
| `cogent work private-note` | Add notes to private (gitignored) DB |
| `cogent work doc-set` | Attach docs to work items |
| `cogent work attest` | Record attestation evidence |
| `cogent work approve/reject` | Approve or reject work |
| `cogent work promote` | Promote to environment |
| `cogent work check` | Record checker results |
| `cogent work hydrate` | Generate deterministic briefing |
| `cogent work discover` | Discover and propose child work |
| `cogent work proposal create/list/show/accept/reject` | Graph mutation proposals |
| `cogent work children` | List child work items |
| `cogent work force-done` | Emergency completion override |
| `cogent work log` | Work item event history |
| `cogent work bootstrap` | Bootstrap work graph from filesystem |
| `cogent inbox` | Quick capture shorthand |
| `cogent capacity` | Show system capacity |

### Infrastructure Commands
| Command | Description |
|---------|-------------|
| `cogent serve` | Start serve runtime (--auto, --port, --host) |
| `cogent adapters` | List installed adapters and capabilities |
| `cogent catalog sync/show/probe` | Provider/model discovery |
| `cogent runtime` | Host-agent inventory |
| `cogent transfer export/run` | Cross-adapter failover |
| `cogent login` | Auth management |
| `cogent version` | Version info |

### Exit Codes
0=success, 1=generic error, 2=invalid invocation, 3=adapter unavailable, 4=auth missing, 5=unsupported, 6=not found, 7=session locked, 8=vendor process failed, 9=timeout, 10=schema error

### CLI→Serve Delegation
When `cogent serve` is running, CLI commands can delegate to the serve HTTP API via `client.go` for operations that need the live service state.

---

## 7. Persistence

### Public SQLite Database (`.cogent/cogent.db`)
21 tables total:

| Table | Purpose |
|-------|---------|
| `sessions` | Canonical sessions (session_id, label, status, origin_adapter, cwd, parent_session_id) |
| `jobs` | Job records (job_id → session_id, work_id, adapter, state, cwd, summary_json) |
| `turns` | Turn records (turn_id → session_id, job_id, input_text, result_summary) |
| `events` | Append-only canonical events (event_id, job_id, seq, ts, kind, phase, payload_json) |
| `native_sessions` | Native vendor session links (session_id, adapter, native_session_id, resumable) |
| `handoffs` | Transfer/handoff records (handoff_id → job_id, session_id, packet_json) |
| `artifacts` | Persisted artifact metadata (artifact_id → job_id, kind, path) |
| `locks` | Session locks (lock_key, adapter, native_session_id, job_id, expires_at) |
| `job_runtime` | Runtime process state (supervisor_pid, vendor_pid, detached, cancel_requested_at) |
| `catalog_snapshots` | Provider/model catalog snapshots (entries_json, issues_json) |
| `work_items` | Work graph items (work_id, title, objective, kind, execution_state, approval_state, lock_state, + 20 metadata fields) |
| `work_edges` | Graph edges (edge_id, from_work_id, to_work_id, edge_type) |
| `work_updates` | Work state update history (update_id, work_id, execution_state, message) |
| `work_notes` | Work item notes (note_id, work_id, note_type, body) |
| `work_proposals` | Graph mutation proposals (proposal_id, proposal_type, state, target_work_id) |
| `attestation_records` | Signed attestation evidence (attestation_id, subject_kind, subject_id, result, method, verifier_kind, signature) |
| `approval_records` | Approval records (approval_id, work_id, status, attestation_ids_json) |
| `promotion_records` | Environment promotions (promotion_id, work_id, environment, status) |
| `doc_content` | In-DB document content (doc_id, work_id, path, body, version, matches_repo) |
| `check_records` | Checker results (check_id, work_id, result, report_json) |

### Private SQLite Database (`.cogent/cogent-private.db`)
1 table (gitignored):
| Table | Purpose |
|-------|---------|
| `private_notes` | Sensitive notes (note_id, work_id, note_type, text, supersedes_note_id) |

### Database Configuration
- WAL mode with `_txlock=immediate` and `_busy_timeout=60000`
- `MaxOpenConns=1`, `MaxIdleConns=1`
- Periodic WAL checkpointing (housekeeping + shutdown)
- Auto-corruption recovery via sqlite3 `.recover`

### `.cogent/` Directory Structure
```
.cogent/
├── cogent.db              # Public SQLite database (tracked in git)
├── cogent-private.db      # Private SQLite database (gitignored)
├── serve.json             # Running serve instance metadata (PID, port)
├── supervisor-brief.md    # Supervisor configuration (adapter, model)
├── supervisor-context.md  # Persistent supervisor memory
├── native-sessions/       # Persisted native adapter session history
├── artifacts/             # Checker artifacts (screenshots, etc.)
├── ca.key / ca.pub        # Ed25519 CA keypair for capability tokens
├── tokens/                # Issued capability token files
├── jobs/                  # Job artifact directories
├── raw/                   # Raw stdout/stderr/native payloads
├── transfers/             # Transfer packet exports
├── debriefs/              # Debrief artifacts
└── worktrees/             # Git worktree state
```

### Migration Model
- Schema is applied via `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` on every open
- No formal migration system — additive schema evolution only
- Legacy `fase`/`cagent` → `cogent` rename migration in `paths.go` (renames directories and DB files)

---

## 8. Skills System

Skills are defined in `skills/cogent/`:
- `SKILL.md` — comprehensive CLI reference for LLM agents (~186 lines), covering when to use cogent, core workflow, all commands
- `checker/` — checker skill definition (checker worker tools and workflow)
- `worker/` — worker skill definition (worker tools and workflow)

Skills define the system prompt and tool surface that agents receive when they're dispatched for specific roles (worker, checker, supervisor). The skill definitions tell the agent what commands are available and how to use them.

---

## 9. Mind Graph

`mind-graph/` is a **Poincaré disk hyperbolic visualization** of the work graph:
- `hyperbolic-proto.html` — standalone HTML + D3.js visualization
- `mind-graph.js` (~74K) — main visualization JavaScript
- `src/` — Vite-based development source
- `vite.config.js` — Vite config with API proxy to cogent serve (port 4242)
- `index.html` — entry point
- Playwright test infrastructure for UI testing

Features: hyperbolic geometry (exponential compression at periphery), force simulation with attention shells, Möbius focus transforms, text LOD, live data from serve API.

The mind-graph is embedded into the cogent binary via `internal/web/embed.go` → `dist/` and served by `cogent serve`.

---

## 10. Build / Run / Test

### Build
```bash
make build        # → build/cogent
make install      # → ~/.local/bin/cogent
make test         # → go test ./internal/...
make lint         # → go vet + staticcheck
```

### Required Environment Variables (names only)
- `ZAI_API_KEY` — ZAI API access
- `AWS_BEARER_TOKEN_BEDROCK` — AWS Bedrock access
- `AWS_REGION` — AWS region (optional)
- `EXA_API_KEY` — Exa web search
- `TAVILY_API_KEY` — Tavily web search
- `BRAVE_API_KEY` — Brave web search
- `SERPER_API_KEY` — Serper web search
- `RESEND_API_KEY` — Resend email service
- `EMAIL_FROM` — Email sender address
- `EMAIL_TO` — Email recipient address
- `COGENT_CONFIG_DIR` — Override config directory
- `COGENT_STATE_DIR` — Override state directory
- `COGENT_CACHE_DIR` — Override cache directory
- `COGENT_AGENT_TOKEN` — Agent capability token file path
- `COGENT_EXECUTABLE` — Override cogent binary path (for recursive tests)

### Scripts
- `scripts/bootstrap-dogfood-web-desktop.sh` — bootstraps a dogfood web/desktop project

### Test Infrastructure
- `testdata/fixtures/` — captured adapter output fixtures
- `testdata/golden/` — golden file test expectations
- `testdata/fake_clis/` — fake vendor CLI scripts (claude, codex, droid, gemini, opencode, pi) for integration testing
- `cmd/cogent/e2e_test.go` (~1685 lines) — comprehensive E2E tests covering all commands
- `cmd/cogent/orchestration_e2e_test.go` — multi-stage pipeline and recursive orchestration tests
- `cmd/cogent/live_e2e_test.go` — live adapter tests (env-gated)
- `eval/` — evaluation tasks (fib-go, fizzbuzz-python, md2html-node) for adapter benchmarking

---

## 11. Integration Points

### Critical Invariants to Preserve
1. **"Agents may always stop, the system may always resume"** — durable state in SQLite, not in-memory
2. **Attestation-gated completion** — work is done only when evidence satisfies policy, not when agent says so
3. **Append-only event stream** — events are never mutated, only appended
4. **One active turn per native session** — lock enforcement prevents concurrent mutations
5. **Canonical lifecycle state machine** — one state machine for all work items, enforced at service layer
6. **Contract precedence: runtime code > docs > DB state** — code is canonical source of truth
7. **Cryptographic agent identity** — Ed25519 CA signs capability tokens; tokens scope what agents can do
8. **Attempt epoch isolation** — stale artifacts from prior attempts can't satisfy new runs
9. **Dedup: MCP retry protection** — 5-second dedup window prevents duplicate work creation

### External Interfaces
1. **CLI (`--json`)** — the primary stable API; machine-readable JSON output on all commands
2. **HTTP API** — RESTful JSON API served by `cogent serve` on configurable port
3. **WebSocket** — real-time event stream for connected UI clients
4. **Embedded Web UI** — mind-graph Poincaré disk visualization
5. **SQLite databases** — public (tracked) + private (gitignored) — the source of truth
6. **Filesystem artifacts** — raw stdout/stderr, transfer packets, debrief artifacts, native session history
7. **Email notifications** — periodic digest emails via Resend API
8. **Adapter CLI subprocesses** — spawned vendor CLIs (claude)
9. **LLM API direct calls** — Anthropic Messages API, OpenAI Responses API (native adapter)
10. **Web search APIs** — Exa, Tavily, Brave, Serper (native adapter tools)

### State That Must Migrate
- All 21 public SQLite tables (sessions, jobs, turns, events, work items, edges, attestations, etc.)
- Private notes table
- `.cogent/` filesystem artifacts (native sessions, raw output, transfers, debriefs, CA keys)
- Config files (TOML)

### Adapter Contracts
- `Adapter` interface (7 methods) — must be reimplemented
- `LiveAgentAdapter` interface (3 methods) — must be reimplemented
- `LiveSession` interface (7 methods) — must be reimplemented
- Tool system (ToolRegistry, ToolFunc) — coding, web, cogent, co-agent tool sets
- Event translation pipeline (raw lines → canonical events)
- History compression strategy

### Key Patterns
- **EventBus pub/sub** — in-process event distribution with `RequiresSupervisorAttention()` filtering
- **WebSocket hub** — broadcast pattern for real-time UI updates
- **Round-robin rotation** — adapter/model selection with history-aware avoidance
- **Proactive history compression** — LLM-based summarization of old turns to fit context windows
- **Co-agent orchestration** — supervisor spawns child sessions, manages via channels
- **Capability tokens** — Ed25519-signed, time-limited, role-scoped tokens for agent authorization
