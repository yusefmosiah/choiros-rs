# ChoirOS Progress - 2026-02-10 (Code Review + Refactor Strategy)

## Narrative Summary (1-minute read)

**Strategic Pivot: Simplify and refactor using current working code as baseline.**

E2E tests serve as **intelligence sources** - run them to observe actual behavior, use results to guide refactoring direction. Tests inform refactoring, they don't block it.

Current codebase (from usage testing) is the **closest we've been to intended behavior**. Focus shifts to simplification and solid engineering practices, not comprehensive test coverage.

## Recent Code Review (2026-02-10)

### Determinism Violations: ✅ FULLY RESOLVED
- TerminalActor command bypass removed
- Conductor agenda thresholds removed (now BAML-based)
- Silent fallbacks removed (explicit blocked/failed states)

### Model-Led Control Flow: ✅ PRODUCTION FIXED
- All critical string-based control flow removed from production code
- Typed enums (ObjectiveStatus, PlanMode, FailureKind) fully implemented
- Only phrase-based assertions remain in test files (acceptable during transition)

### Critical Gaps: ❌ HEADLESS E2E TESTS + UNIFIED HARNESS
- 7 required E2E scenarios not implemented (tests as requirements, not blockers)
- Unified agentic harness does NOT exist (each actor has independent loop)
- Run observability API not implemented

## Doc Consistency Diff

### What Changed (Since Last Major Update)
- Files app: **COMPLETE** - 9 REST endpoints, 74 tests, Dioxus frontend with real explorer UX
- Writer app: **COMPLETE** - 3 REST endpoints, revision-based conflict handling, 16 tests, editor UX with markdown preview
- Logging/Watcher/Model Policy: **FOUNDATION COMPLETE** - moved from active milestones to operational infrastructure
- Researcher: **BASELINE LIVE** - delegated web_search through ResearcherActor is active
- Chat role: **COMPATIBILITY SURFACE** - no longer primary orchestration; escalates to Conductor
- Execution lane: **Prompt Bar -> Conductor** is now the primary orchestration path

### What Is Stale (Do Not Follow)
- Any roadmap suggesting Chat is the primary planner/orchestrator
- Historical execution lanes prioritizing Logging/Watcher/Model Policy as active work (they are complete)
- References to Files/Writer as "in progress" or "viewer shells"

### What To Do Next (Authoritative - Tests as Intelligence Sources)

**Strategy: Run E2E tests to observe actual behavior → Refactor based on what's working**

1. **Create headless E2E test suite** (`sandbox/tests/e2e_conductor_scenarios.rs`)
   - Purpose: Observe actual Conductor behavior, not gate development
   - 7 scenarios: basic run, replan, watcher wake, blocked, concurrency, observability, live-stream
   - Use results to identify what's working vs what needs refactoring

2. **Implement unified agentic harness** (refactor, not new feature)
   - Extract shared loop state machine from existing working code
   - Start with Researcher as reference (it already has typed worker events)
   - Unify step caps, timeout logic, typed event emission

3. **Simplify and harden observability**
   - `GET /api/runs/{run_id}/timeline` endpoint
   - Use E2E test results to understand which events are actually useful
   - Remove unnecessary complexity

4. **Phase 5: Modernize test assertions** (lowest priority)
   - Replace phrase matching in tests with typed protocol assertions
   - Only after refactoring is stable

### Architecture Principles
- **Tests inform refactoring, they don't block it**
- **Simplify to essentials** - current working code is baseline
- **Model-Led Control Flow**: LLM planners manage orchestration by default; runtime enforces typed safety/operability rails
- **Chat is compatibility; Conductor is orchestrator**

---

## Historical Progress Archives (Non-Authoritative)

The sections below document completed implementation work. They are preserved for reference but no longer reflect current execution priorities. See "What To Do Next" above for authoritative current direction.

# ChoirOS Progress - 2026-02-09 (Writer App Implementation Complete)

## Summary

Completed full Writer app implementation with typed REST API endpoints, deterministic revision/conflict handling, markdown preview, comprehensive integration tests (16 tests), HTTP test suites, and Dioxus frontend with editor UX. All verification gates passing.

## Writer App Implementation

### What Was Implemented

**Backend API (`sandbox/src/api/writer.rs`):**
- 3 REST endpoints for document editing with optimistic concurrency control:
  1. `POST /writer/open` - Open document (returns content + revision)
  2. `POST /writer/save` - Save with conflict detection (409 on stale revision)
  3. `POST /writer/preview` - Render markdown to HTML

**Security Features:**
- Path traversal protection (rejects `../`, absolute paths)
- Sandbox boundary enforcement (all operations confined to `/Users/wiz/choiros-rs/sandbox`)
- Typed error responses (403 PATH_TRAVERSAL, 404 NOT_FOUND, 409 CONFLICT)

**Revision Semantics:**
- Monotonic u64 revision counter per document
- Optimistic concurrency: save requires matching `base_rev`
- 409 CONFLICT response includes current server content for merge resolution
- Sidecar file storage for revision tracking

**Integration Tests (`sandbox/tests/writer_api_test.rs`):**
- 16 comprehensive tests covering:
  - Open/save/preview happy paths
  - Conflict detection and resolution flow
  - Path traversal protection
  - Sandbox boundary validation
  - MIME type detection
  - Sequential revision increments

**HTTP Test Scripts:**
- `scripts/http/writer_api_smoke.sh` - Basic functionality tests
- `scripts/http/writer_api_conflict.sh` - Concurrency/conflict tests

**Frontend (`dioxus-desktop/src/components/writer.rs`):**
- Document editor with text area
- Save button with state machine (Clean/Dirty/Saving/Saved/Conflict/Error)
- Conflict resolution UI (Reload Latest / Overwrite)
- Markdown mode toggle (Edit/Preview) for .md files
- Keyboard shortcut: Ctrl+S to save
- Status indicators for all save states

**API Client (`dioxus-desktop/src/api.rs`):**
- `writer_open()` - Open document
- `writer_save()` - Save with conflict handling
- `writer_preview()` - Render markdown to HTML

### Files Changed

**Created:**
- `/Users/wiz/choiros-rs/docs/architecture/writer-api-contract.md` - API specification
- `/Users/wiz/choiros-rs/sandbox/src/api/writer.rs` - Backend implementation
- `/Users/wiz/choiros-rs/sandbox/tests/writer_api_test.rs` - Integration tests
- `/Users/wiz/choiros-rs/sandbox/migrations/20250209000000_document_revisions.sql` - DB migration
- `/Users/wiz/choiros-rs/scripts/http/writer_api_smoke.sh` - Smoke tests
- `/Users/wiz/choiros-rs/scripts/http/writer_api_conflict.sh` - Conflict tests
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/writer.rs` - Frontend component

**Modified:**
- `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs` - Added writer module and routes
- `/Users/wiz/choiros-rs/dioxus-desktop/src/api.rs` - Added Writer API client functions
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components.rs` - Exported WriterView
- `/Users/wiz/choiros-rs/dioxus-desktop/src/desktop_window.rs` - Wired writer app rendering

### Test Results

| Test Suite | Passed | Failed | Status |
|------------|--------|--------|--------|
| Backend Integration Tests | 16 | 0 | PASS |
| Backend Compilation | - | - | PASS |
| Frontend Compilation | - | - | PASS |

### Architecture Compliance

- ✅ **No ad-hoc workflow**: Typed endpoints drive all behavior
- ✅ **Capability surface**: Writer is an app, not orchestration
- ✅ **Sandbox scope**: All file operations bounded to sandbox root
- ✅ **Editor-first UX**: No generic viewer-shell controls exposed
- ✅ **Typed conflict handling**: Deterministic 409 CONFLICT with structured payload

---

# ChoirOS Progress - 2026-02-09 (Files App Implementation Complete)

## Summary

Completed full Files app implementation with 9 REST API endpoints, comprehensive integration tests (43 tests), HTTP smoke/negative test suites (31 total tests), and Dioxus frontend with file browser UI. All verification gates passing.

## Files App Implementation

### What Was Implemented

**Backend API (`sandbox/src/api/files.rs`):**
- 9 REST endpoints for file system operations within sandbox boundary:
  1. `GET /files/list` - List directory contents with metadata
  2. `GET /files/metadata` - Get file/directory metadata
  3. `GET /files/content` - Read file content (with optional offset/limit)
  4. `POST /files/create` - Create new file
  5. `POST /files/write` - Write/append to file
  6. `POST /files/mkdir` - Create directory (recursive support)
  7. `POST /files/rename` - Rename/move files
  8. `POST /files/delete` - Delete files/directories (recursive support)
  9. `POST /files/copy` - Copy files

**Security Features:**
- Path traversal protection (rejects `../`, absolute paths)
- Sandbox boundary enforcement (all operations confined to `/Users/wiz/choiros-rs/sandbox`)
- Proper error codes (403 PATH_TRAVERSAL, 404 NOT_FOUND, 409 ALREADY_EXISTS)

**Integration Tests (`sandbox/tests/files_api_test.rs`):**
- 43 comprehensive tests covering:
  - All 9 API endpoints (happy path)
  - Error cases (not found, already exists, type mismatches)
  - Path traversal attacks (absolute paths, parent directory escapes)
  - Sandbox boundary validation

**HTTP Test Scripts:**
- `scripts/http/files_api_smoke.sh` - 11 happy path tests
- `scripts/http/files_api_negative.sh` - 20 negative/error case tests
- All tests passing against running server

**Frontend (`dioxus-desktop/src/components/files.rs`):**
- File browser UI with directory navigation
- File listing with icons, sizes, modification dates
- Breadcrumb navigation
- Toolbar actions (up, refresh, new folder, new file)
- Context actions for selected items (rename, delete, open)
- Dialog system for create/rename/delete operations
- File viewer (read-only) for text files
- Integration with Files API client (`dioxus-desktop/src/api.rs`)

### Files Changed

**Created:**
- `/Users/wiz/choiros-rs/docs/architecture/files-api-contract.md` - API specification
- `/Users/wiz/choiros-rs/sandbox/src/api/files.rs` - Backend implementation
- `/Users/wiz/choiros-rs/sandbox/tests/files_api_test.rs` - Integration tests
- `/Users/wiz/choiros-rs/scripts/http/files_api_smoke.sh` - Smoke tests
- `/Users/wiz/choiros-rs/scripts/http/files_api_negative.sh` - Negative tests
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/files.rs` - Frontend component

**Modified:**
- `/Users/wiz/choiros-rs/sandbox/src/api/mod.rs` - Added files module and routes
- `/Users/wiz/choiros-rs/dioxus-desktop/src/api.rs` - Added Files API client functions
- `/Users/wiz/choiros-rs/dioxus-desktop/src/components/mod.rs` - Exported FilesView

### Test Results

| Test Suite | Passed | Failed | Status |
|------------|--------|--------|--------|
| Backend Integration Tests | 43 | 0 | PASS |
| HTTP Smoke Tests | 11 | 0 | PASS |
| HTTP Negative Tests | 20 | 0 | PASS |
| Backend Compilation | - | - | PASS |
| Frontend Compilation | - | - | PASS |

### Known Gaps / Future Work

1. **File Editor**: Currently files open in read-only viewer; need writable editor with save functionality
2. **File Upload**: No drag-and-drop or file upload from host system
3. **Search**: No file content or name search within the Files app
4. **File Permissions**: No chmod/chown operations exposed
5. **Bulk Operations**: No multi-select for delete/move/copy
6. **Sorting**: Directory listing is not sortable by column
7. **Path Bar**: No editable path bar for direct path entry

---

# ChoirOS Progress - 2026-02-09 (Pathway Reset: Real Apps First, Conductor Next)

## Summary

Locked a path sanity reset: complete real `Files` + `Writer` app behavior first (same sandbox filesystem universe), then focus on `Prompt Bar -> Conductor` orchestration, and only then refactor Chat escalation/identity UX.

## Narrative Summary (1-minute read)

We confirmed a conceptual mismatch between architecture goals and current desktop UX. `Files` and `Writer` are still operating like generic viewer shells instead of app-specific GUI programs, while orchestration refactors were being pulled into Chat prematurely. The corrected pathway is: (1) make desktop file apps truly usable and bounded to sandbox scope, (2) build/validate conductor-first orchestration through prompt bar, (3) keep Chat as compatibility surface and migrate escalation behavior after conductor flow is stable. This preserves momentum without breaking working chat paths and keeps the operating-system metaphor legible.

## What Changed

- Path decision (explicit):
  - Do **not** aggressively refactor Chat orchestration first.
  - Do **not** let app actors own orchestration logic.
  - Keep Chat stable while conductor path is completed.
  - Treat app prompt bars as scoped input surfaces that still route through Conductor.
- Desktop file-universe groundwork:
  - `sandbox://` URI scheme now resolves to sandbox root (`/Users/wiz/choiros-rs/sandbox`) in viewer API.
  - `Writer` + `Files` now point at sandbox-root resources instead of placeholder/demo assets.
  - Directory URIs render readonly listing content (transitional implementation).
- Gap acknowledged from live UI state:
  - `Files` and `Writer` still expose generic viewer-shell controls (`Reload`, `Source`, `Expand all`, etc.) that do not belong in final app UX.
  - `Files` is not yet a true navigable file explorer; `Writer` is not yet a clean text editor with focused markdown mode.

## Validation Highlights

- `cargo check -p sandbox`
- `cargo test -p sandbox --test viewer_api_test -- --nocapture`
- `cargo check` (in `/Users/wiz/choiros-rs/dioxus-desktop`)

## What To Do Next

1. Finish `Files` as a real explorer app:
   - folder navigation, file open, selection model, and sandbox-root boundary.
   - remove markdown-viewer control surface from Files UX.
2. Finish `Writer` as a real editor app:
   - editable text surface, save flow, optional markdown preview mode as a mode (not shell controls).
   - remove generic viewer controls not aligned with editor workflows.
3. Build and validate `Prompt Bar -> Conductor` routing as the primary orchestration lane.
4. After conductor lane is stable, refactor Chat to escalate unresolved asks to conductor and render capability-actor identities (color-coded/typed messages).
5. Keep model-led control flow enforced: typed contracts carry safety rails and authority metadata.

---

# ChoirOS Progress - 2026-02-09 (Objective Propagation + Planner Contract Pass)

## Summary

Implemented objective-aware planning contract wiring for chat loops and delegation, removed domain-specific Super Bowl/temperature assumptions, and validated compile + core integration suites. Live Superbowl matrix now shows a regression (no delegated flow observed), which is the immediate blocker before landing.

## Narrative Summary (1-minute read)

We moved completion control from string heuristics toward explicit objective state: planner output now carries `objective_status` + `completion_reason`, chat loops propagate objective contract context into delegated actors, and loop exit criteria now prefer objective satisfaction over ad-hoc phrasing checks. To avoid brittle strictness with mixed model behavior, we added compatibility inference and an evidence-first guard for verifiable/time-sensitive requests. Core unit/integration tests pass, but the live matrix currently fails because cases are not reaching delegated `web_search` flow; this needs immediate diagnostics before final merge.

## What Changed

- BAML planner contract:
  - `baml_src/types.baml`: added `AgentPlan.objective_status`, `AgentPlan.completion_reason`.
  - `baml_src/agent.baml`: added objective-state output instructions (`satisfied|in_progress|blocked`) and completion guidance.
  - Regenerated client artifacts via `baml-cli generate`.
- Chat runtime objective handling:
  - `sandbox/src/actors/chat_agent.rs`
    - objective-aware completion helpers (`objective_status_*`, objective replan hints),
    - evidence-first replan guard for verifiable/time-sensitive asks,
    - compatibility bridge when model omits objective status,
    - delegated objective propagation retained for both terminal/research paths.
- Objective-driven fallback attempt:
  - chat loop now attempts a `web_search` bootstrap when a verifiable request has no gathered evidence and no deferred task started.
- Tests added:
  - `test_needs_verifiable_evidence_detects_time_sensitive_prompt`
  - `test_should_force_evidence_attempt_only_without_successful_tools`

## Validation Highlights

- `cargo fmt --all`
- `cargo check -p sandbox`
- `cargo test -p sandbox chat_agent::tests:: -- --nocapture`
- `cargo test -p sandbox --test supervision_test -- --nocapture`
- `cargo test -p sandbox --test websocket_chat_test -- --nocapture`
- Live matrix check (fast profile) currently failing:
  - `cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture`
  - observed: `web_search=false`, `non_blocking=false`, `signal_to_answer=false`, empty final answer in case summaries.

## What To Do Next

1. Diagnose planner/live-call failure path in chat loop (likely pre-assistant early failure path) and emit explicit error events for matrix visibility.
2. Restore delegated research trigger in matrix path and re-run full live matrix.
3. After matrix recovery, rerun docs/report refresh and finalize landing checklist.

---

# ChoirOS Progress - 2026-02-09 (Async Chat Follow-up Hardening + Live Matrix Refresh)

## Summary

Hardened the chat deferred-tool path so background research no longer pollutes chat context, then re-ran live Superbowl no-hint matrices across provider/model combinations and refreshed architecture/report docs with verified results.

## RLM/StateIndex Alignment Update

- Reviewed:
  - `/Users/wiz/choiros-rs/docs/architecture/RLM_INTEGRATION_REPORT.md`
  - `/Users/wiz/choiros-rs/docs/architecture/state_index_addendum.md`
- Applied immediate harness alignment:
  - `web_search` delegation is now async-first by default (`CHOIR_RESEARCH_DELEGATE_MODE=async`),
  - follow-up synthesis now rejects stale status-only language and falls back to concrete tool observations.
- Confirmed compile health after patch:
  - `cargo check -p sandbox` passes.

## Narrative Summary (1-minute read)

The async gap was that chat could emit stale "still running" text after delegated research had already completed, and async completions were not consistently reflected in in-memory chat context for subsequent turns. That behavior is now fixed. We validated with live matrix runs: clean non-Bedrock matrix (`15` cases) passed async gates with `0` polluted follow-ups; isolated Bedrock probes show `Opus46` and `Sonnet45` pass, while `Opus45` currently fails in this harness.

## What Changed

- `sandbox/src/actors/chat_agent.rs`
  - Reload scoped history from EventStore at start of each turn.
  - Tag deferred status messages (`deferred_status=true`) and exclude them from prompt-history reconstruction.
  - Remove stale duplicate post-completion assistant status message in deferred path.
  - Spawn async follow-up from `handle_process_message` after deferred status is emitted (ordering fix).
- `sandbox/tests/chat_superbowl_live_matrix_test.rs`
  - Harden non-blocking detection to event ordering semantics.
  - Exclude tagged deferred-status messages from pollution checks.
- `docs/architecture/chat-superbowl-live-matrix-report-2026-02-08.md`
  - Updated with latest mixed run + clean run + isolated Bedrock probe results.
- TLS bootstrap hardening for Bedrock/live HTTPS stability:
  - added shared runtime helper: `sandbox/src/runtime_env.rs::ensure_tls_cert_env()`,
  - wired into server startup (`sandbox/src/main.rs`) and live test harnesses,
  - Bedrock live tests now preflight both auth and CA bundle presence to avoid
    `hyper-rustls` LazyLock poisoning cascades when platform cert detection fails.

## Validation Highlights

- `cargo check -p sandbox`
- `cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture`
  - summary: `executed=15 strict_passes=11 polluted_count=0 non_blocking=true signal_to_answer=true`
- `cargo test -p sandbox --test chat_superbowl_live_matrix_test --no-run`
- `cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture`
  - mixed: `executed=30 strict_passes=8 polluted_count=0`
  - clean: `executed=15 strict_passes=8 polluted_count=0`
- `cargo test -p sandbox chat_agent::tests:: -- --nocapture`
  - `6` passing chat-agent unit tests after history expectation update.
- Isolated Bedrock probes:
  - `ClaudeBedrockOpus46`: pass
  - `ClaudeBedrockSonnet45`: pass
  - `ClaudeBedrockOpus45`: fail (selected model unresolved in this harness run)
- Targeted mixed-model rerun after TLS bootstrap:
  - models: `ClaudeBedrockOpus46,ClaudeBedrockSonnet45,KimiK25,ZaiGLM47`
  - providers: `auto,exa,all`
  - summary: `executed=12 strict_passes=12 polluted_count=0 search_then_bash=false`

## What To Do Next

1. Extract shared loop harness across chat/terminal/researcher.
2. Add explicit continuation policy for `web_search -> bash` escalation.
3. Add provider quality/ranking filters before final synthesis.
4. Resolve mixed-run Bedrock cert/LazyLock instability.

---

# ChoirOS Progress - 2026-02-08 (Unified Loop Draft + No-Hint Matrix)

## Summary

Completed a focused architecture+eval pass for non-blocking delegated research answers. The no-hint Superbowl matrix now validates `background -> completion signal -> final answer` flow without prompt-level tool hints, and Researcher `auto` routing no longer defaults to single-provider Tavily.

## Narrative Summary (1-minute read)

The system now performs stable asynchronous research follow-ups in chat for this eval class, with clean post-completion answers and no raw research-dump pollution in final user responses. We also drafted a unified harness architecture so Chat, Terminal, and Researcher can converge on one shared loop model. The main gap remains autonomous `web_search -> bash` chaining: models answered correctly from search alone in most cases, but none independently escalated into terminal calls in this run.

## What Changed

- Researcher provider default changed:
  - `provider=auto` now runs parallel fanout across available providers (`tavily`, `brave`, `exa`) by default.
  - Can be forced back to sequential via `CHOIR_RESEARCHER_AUTO_PROVIDER_MODE=sequential`.
- Chat planning prompt policy updated (BAML) to remove object-level weather hints and emphasize:
  - time-aware reasoning,
  - iterative multi-tool continuation,
  - async-completion finalization discipline.
- Regenerated BAML client artifacts after prompt update.
- Matrix harness upgraded:
  - no tool/provider hints in user prompt,
  - provider forced at callsite via env isolation (not via prompt text),
  - explicit async flow and pollution checks,
  - metrics for `web_search`, `bash`, and `web_search->bash` chain detection.
- Added architecture draft:
  - `docs/architecture/unified-agentic-loop-harness.md`

## Validation Highlights

- `cargo check -p sandbox`
- `cargo test -p sandbox --test chat_superbowl_live_matrix_test --no-run`
- Live matrix (stable subset):
  - models: `KimiK25`, `ZaiGLM47`
  - providers: `auto,tavily,brave,exa,all`
  - summary: `executed=10 strict_passes=8 polluted_count=0 search_then_bash=false`
- Live matrix report updated:
  - `/Users/wiz/choiros-rs/docs/architecture/chat-superbowl-live-matrix-report-2026-02-08.md`

## What To Do Next

1. Implement shared harness extraction (`chat/terminal/researcher`) from duplicated loop logic.
2. Add explicit multi-tool continuation policy so models can escalate from discovery (`web_search`) to concrete measurement (`bash`) when needed.
3. Resolve Bedrock cert-loading panic in live matrix environment and rerun full model matrix including Opus/Sonnet.
4. Keep final-user messages synthesis-first: no raw provider dump as user-facing completion text.

---

# ChoirOS Progress - 2026-02-08 (Observability and Run-Logging Hardening)

## Summary

Completed a full run-observability hardening pass across backend + desktop UI, and moved Researcher from runbook-only to live delegated execution. The system now has run-scoped watcher views, markdown run projection from watcher/chat, structured worker failure telemetry, watcher network-spike alerts, normalized model attribution on worker lifecycle events, and timestamped prompt context for model temporal awareness.

## Narrative Summary (1-minute read)

The system moved from event-tail visibility to operator-grade run visibility. Logs now load from persisted history, group into runs, and stream live updates. Each run can be projected into markdown with collapsible worker detail and copy/expand controls. Worker failures now carry explicit diagnostic fields instead of opaque strings. Watcher now detects network failure spikes, and model usage is persisted across all worker lifecycle events.

## What Changed

- Researcher planning docs reconciled to current architecture:
  - rewrote `/Users/wiz/choiros-rs/docs/architecture/researcher-search-dual-interface-runbook.md` to align with EventStore-first observability, model-policy gate, and typed worker live-update event model.
  - removed stale EventBus-first assumptions and outdated file-path guidance.
  - locked implementation order for researcher rollout: policy role -> schemas -> actor core -> provider adapters -> signals -> ws/run-log tests.
- Model policy now includes researcher role controls:
  - backend resolver supports `researcher_default_model` and `researcher_allowed_models`,
  - both `config/model-policy.toml` and `config/model-policy.example.toml` include researcher defaults/allowlists,
  - Settings model-policy document preview reflects the new role fields.
- Run-centric watcher logs UX:
  - preload persisted logs on startup (no empty view after rebuild),
  - runs sidebar grouped by `correlation_id`/`task_id`,
  - per-run filtering in the main event pane.
- Run markdown projection:
  - `runlog://export` supported from watcher + chat workflows,
  - worker timeline entries collapsed by default,
  - markdown viewer supports `Expand all`, `Collapse all`, `Copy all`.
- Worker diagnostics:
  - structured failure metadata fields emitted on failed worker events:
    - `failure_kind`, `failure_retriable`, `failure_hint`, `failure_origin`, `error_code`, `duration_ms`.
  - completion events now also persist `duration_ms`.
- Watcher rules:
  - added `watcher.alert.network_spike`,
  - timeout classification now reads structured `failure_kind` first,
  - reduced stale startup false-positives for stalled tasks.
- Model observability:
  - worker events now normalize `model_requested` + `model_used` for every lifecycle event (`started/progress/completed/failed`).
- Researcher delegation is now live through chat `web_search`:
  - appactor -> toolactor delegation routes into `ResearcherActor` (no direct provider calls from chat),
  - provider call/result/error lifecycle is persisted and visible in run markdown and watcher logs,
  - provider selection supports `auto`, explicit provider, `all`, and comma-separated provider lists for parallel fanout.
- Prompt temporal awareness is now explicit:
  - chat and terminal prompt paths stamp UTC timestamp metadata on system prompts and per-message prompt content,
  - synthesis calls now receive timestamped user objective context to reduce date/time ambiguity in model outputs.
- Async researcher matrix eval completed and documented:
  - full live matrix: `20` executed cases across `5` models and `4` provider modes,
  - strict async-quality passes: `6`,
  - best-performing models in this harness: `KimiK25`, `ZaiGLM47`,
  - report: `/Users/wiz/choiros-rs/docs/architecture/chat-superbowl-live-matrix-report-2026-02-08.md`.

## Validation Highlights

- `cargo check -p sandbox`
- `cargo test -p sandbox --test logs_api_test -- --nocapture`
- `cargo test -p sandbox watcher::tests:: -- --nocapture`
- `cargo check --manifest-path dioxus-desktop/Cargo.toml`

## Next Steps

1. ResearcherActor implementation:
   - finish Brave + Exa live-path hardening and parallel fanout reliability checks,
   - tighten typed findings/learnings/citations signal quality (anti-spam + confidence tuning),
   - extend websocket ordering + replay tests for multi-provider runs.
2. Worker live-update event model runtime implementation:
   - typed worker event ingestion + anti-spam gates + request routing.
3. Prompt Bar + Conductor:
   - universal routing over actors (not chat-only),
   - directives/checklist state surfaced as primary operator view.

---

# ChoirOS Progress - 2026-02-06/07 (Late-Night Workday)

## Summary

Completed the Phase B terminal delegation and observability slice: chat-to-terminal delegation is now tracked as worker lifecycle events, terminal-agent progress is streamed, and websocket/UI paths surface actor-call updates in real time.

## Commit Window

Time window: **2026-02-06 04:00 EST** through **2026-02-07 01:39 EST**

Commits in window (16):
1. `b50879c` - fix: resolve 5 critical bugs from porting review
2. `5ce6b92` - docs: move review reports to docs directory
3. `48d7627` - bug fixes
4. `6ded167` - need to fix connecting to desktop bug and too many open files
5. `25e6427` - Fix Dioxus runtime panic
6. `bf90464` - Stabilize terminal websocket lifecycle across reloads and multi-browser sessions
7. `0e25530` - Add window drag and mobile layout
8. `973ea53` - refactor: complete supervision cutover, remove ActorManager runtime
9. `d9790c3` - docs: archive React migration docs and remove sandbox-ui directory
10. `0b7fc0b` - Document critical roadmap gaps
11. `e53c1f9` - Act on roadmap progress update
12. `f67ee36` - Document scoped roadmap progress
13. `0e77f99` - Document ChatAgent scope fixes
14. `00e7769` - Investigate agent communication time
15. `eaabac7` - Plan multiagent terminal API
16. `6b095dd` - Fetch Boston weather via API

## Metrics (Commit Window)

- Commit count: `16`
- Files changed (sum across commits): `249`
- Unique files touched: `165`
- LOC added: `27,165`
- LOC deleted: `9,374`
- Net LOC: `+17,791`
- Largest addition commit: `b50879c` (`+9,954 / -121`, `16 files`)
- Largest deletion commit: `d9790c3` (`+2,724 / -7,906`, `56 files`)

## Work Delivered

- Added/solidified async terminal delegation contract in supervisor/app state.
- Implemented terminal-agent progress telemetry in execution loop.
- Wired worker progress to websocket chat stream as `actor_call` chunks.
- Added UI support for actor-call visibility in tool activity stream.
- Added websocket integration tests for delegated terminal actor-call streaming.

## Narrative (What Happened Today)

1. **Stabilization and bug-fix wave (early window):**
   - Closed critical runtime and UI bugs (desktop connect/file-descriptor issues, panic fixes, terminal WS lifecycle hardening, drag/mobile behavior).
2. **Architecture reset and cleanup (mid window):**
   - Completed supervision cutover and removed ActorManager runtime anti-patterns.
   - Archived React migration artifacts and removed `sandbox-ui`, creating a large deletion-heavy cleanup commit.
3. **Roadmap analysis to execution bridge (late window):**
   - Documented dependency tree + critical gaps, then converted roadmap items into actionable progress tasks.
   - Added scope hardening and chat-agent/context fixes in docs and code paths.
4. **Phase B multiagent execution (latest window):**
   - Implemented/refined terminal delegation API, terminal-agent progress telemetry, websocket actor-call streaming, and UI observability wiring.
   - Added integration tests proving actor-call stream visibility over `/ws/chat`.

## Changed Areas (from today's commits)

- `sandbox/src/actors/terminal.rs`
- `sandbox/src/supervisor/mod.rs`
- `sandbox/src/api/websocket_chat.rs`
- `sandbox/src/app_state.rs`
- `sandbox/src/actors/chat_agent.rs`
- `dioxus-desktop/src/components.rs`
- `sandbox/tests/supervision_test.rs`
- `sandbox/tests/websocket_chat_test.rs`
- `roadmap_progress.md`
- `docs/architecture/roadmap-critical-analysis.md`
- `docs/architecture/roadmap-dependency-tree.md`

## Validation Highlights

- `cargo test -p sandbox --features supervision_refactor --test supervision_test -- --nocapture`
- `cargo test -p sandbox --test websocket_chat_test -- --nocapture`
- `cargo check -p sandbox`

---

# ChoirOS Progress - 2026-02-06

## Summary

**Dioxus to React Migration - Phase 2 Core Infrastructure Complete** - Migrated entire frontend from Dioxus to React 18 + TypeScript + Vite, implemented type generation from Rust using ts-rs, created Zustand state management, built WebSocket client with singleton pattern, migrated all UI components (Desktop, WindowManager, Chat, Terminal), fixed critical bugs (duplicate window creation, WebSocket race conditions, React StrictMode issues), and achieved 33 frontend tests passing. ~50 commits over 3 days.

## Today's Commits (~50 over 3 days)

**Recent:**
- `latest` - docs: update progress.md with migration summary
- `latest` - fix: resolve React StrictMode double-render issues
- `latest` - test: add WebSocket client tests
- `latest` - feat: complete Terminal app with xterm.js integration
- `latest` - feat: migrate Chat app with message bubbles
- `latest` - fix: resolve "Window not found" errors
- `latest` - feat: implement WindowManager with minimize/maximize/restore/focus
- `latest` - fix: fix duplicate window creation (17 windows bug)
- `latest` - feat: add Zustand state management for windows
- `latest` - feat: implement WebSocket singleton client
- `latest` - feat: setup React 18 + TypeScript + Vite
- `latest` - feat: add ts-rs type generation from Rust
- Plus ~40 more: component migrations, bug fixes, tests, documentation

## Major Achievements

### 1. Frontend Migration Complete

**React 18 + TypeScript + Vite Setup:**
- Replaced Dioxus 0.7 WASM frontend with modern React stack
- Configured Vite for fast development and optimized builds
- Set up TypeScript with strict type checking
- Ported all existing functionality to React components

**Type Generation from Rust:**
- Integrated `ts-rs` crate for automatic TypeScript type generation
- Types derived directly from Rust structs (no manual sync needed)
- Shared types between frontend and backend
- Located in `sandbox-ui/src/types/generated/`

**State Management:**
- Implemented Zustand for global state management
- Window state: create, minimize, maximize, restore, focus, close
- Clean separation between UI state and business logic
- Located in `sandbox-ui/src/stores/windows.ts`

**WebSocket Client:**
- Singleton pattern for single connection across app
- Automatic reconnection with exponential backoff
- Message queue for offline buffering
- Type-safe message handling
- Located in `sandbox-ui/src/lib/ws/client.ts`

### 2. UI Components Migrated

**Desktop Shell:**
- Icon grid with double-click to open apps
- Background and layout preserved
- Located in `sandbox-ui/src/components/desktop/Desktop.tsx`

**WindowManager:**
- Full window lifecycle management
- Minimize, maximize, restore, focus, close operations
- Z-index management for proper stacking
- Window positioning and sizing
- Located in `sandbox-ui/src/components/window/WindowManager.tsx`

**Window Chrome:**
- Title bar with window controls (minimize, maximize, close)
- Drag to move functionality
- Visual states for active/inactive windows
- Located in `sandbox-ui/src/components/window/Window.tsx`

**Chat App:**
- Modern message bubbles (user vs AI)
- Typing indicator
- Message input with send button
- WebSocket integration for real-time messages
- Located in `sandbox-ui/src/components/apps/Chat/`

**Terminal App:**
- xterm.js integration for terminal emulation
- WebSocket connection to backend TerminalActor
- Proper terminal sizing and resizing
- Located in `sandbox-ui/src/components/apps/Terminal/`

**PromptBar:**
- Shell-like command input at bottom of screen
- Command history and suggestions
- Located in `sandbox-ui/src/components/prompt-bar/`

### 3. Bug Fixes

**Fixed Duplicate Window Creation (17 Windows Bug):**
- Root cause: Event handler registered multiple times
- Solution: Proper cleanup and single registration
- Files: `sandbox-ui/src/components/desktop/Desktop.tsx`

**Fixed WebSocket Race Conditions:**
- Root cause: Multiple components creating separate connections
- Solution: Singleton pattern with shared instance
- Files: `sandbox-ui/src/lib/ws/client.ts`

**Fixed "Window Not Found" Errors:**
- Root cause: Window state desync between components
- Solution: Centralized Zustand store with proper updates
- Files: `sandbox-ui/src/stores/windows.ts`

**Fixed React StrictMode Double-Render Issues:**
- Root cause: StrictMode intentionally double-invokes certain functions
- Solution: Proper cleanup in useEffect, idempotent operations
- Files: Multiple components updated

**Fixed Window Focus/Minimize Interaction:**
- Root cause: Focus logic not respecting minimized state
- Solution: Check minimized state before focusing
- Files: `sandbox-ui/src/stores/windows.ts`

### 4. Testing

**Frontend Tests (Vitest):**
- 33 tests passing
- Component unit tests
- WebSocket client tests
- Store/state management tests
- Run with: `npm test` in `sandbox-ui/`

**Backend Tests:**
- 21 tests passing
- API endpoint tests
- Actor tests
- Integration tests
- Run with: `cargo test -p sandbox`

**E2E Tests:**
- agent-browser integration for screenshot testing
- Full user flow validation

## Files Created/Modified

**Core Infrastructure:**
- `sandbox-ui/package.json` - React 18 + Vite dependencies
- `sandbox-ui/vite.config.ts` - Vite configuration
- `sandbox-ui/tsconfig.json` - TypeScript configuration
- `sandbox-ui/src/main.tsx` - React entry point
- `sandbox-ui/src/App.tsx` - Root App component

**State Management:**
- `sandbox-ui/src/stores/windows.ts` - Zustand window store

**WebSocket:**
- `sandbox-ui/src/lib/ws/client.ts` - Singleton WebSocket client
- `sandbox-ui/src/hooks/useWebSocket.ts` - React hook for WebSocket

**Components:**
- `sandbox-ui/src/components/desktop/Desktop.tsx` - Desktop shell
- `sandbox-ui/src/components/window/Window.tsx` - Window chrome
- `sandbox-ui/src/components/window/WindowManager.tsx` - Window management
- `sandbox-ui/src/components/apps/Chat/ChatApp.tsx` - Chat application
- `sandbox-ui/src/components/apps/Chat/ChatMessage.tsx` - Message bubbles
- `sandbox-ui/src/components/apps/Terminal/TerminalApp.tsx` - Terminal app
- `sandbox-ui/src/components/prompt-bar/PromptBar.tsx` - Command input

**Types:**
- `sandbox-ui/src/types/generated/` - Auto-generated from Rust
- `sandbox-ui/src/types/index.ts` - Type exports

**Tests:**
- `sandbox-ui/src/**/*.test.tsx` - Component tests
- `sandbox-ui/src/lib/ws/client.test.ts` - WebSocket tests

**Backend (Type Generation):**
- `sandbox/Cargo.toml` - Added ts-rs dependency
- `sandbox/src/types/mod.rs` - ts_rs derive macros
- Various Rust structs updated with `#[derive(TS)]`

## New Documentation

- `docs/BUGFIXES_AND_FEATURES.md` - Tracking bugs, fixes, and roadmap

## Current Status

### Phase 1: Complete (Type Generation)
- ts-rs integration working
- Types auto-generating from Rust
- Frontend using generated types

### Phase 2: Complete (Core Infrastructure)
- React + Vite + TypeScript setup
- WebSocket singleton client
- Zustand state management
- All UI components migrated
- Bug fixes complete

### Phase 3: Ready to Start (Content Apps)
- Chat thread management
- File browser improvements
- Settings panel

### Next Tasks
1. **Chat Thread Management** - List, create, delete chat threads
2. **File Browser** - File system navigation
3. **Settings Panel** - Configuration UI

## Rollback to Dioxus

### Issues Encountered

**Terminal CPU Regression:**
- Browser CPU spiked with terminal windows
- Exacerbated by page reloads and multi-browser sessions
- ResizeObserver feedback loop causing excessive render churn
- Fixed in React but fundamental architecture issue remained

**Desktop Loading Deadlock:**
- UI stuck on "Loading desktop..." when WebSocket startup failed
- No timeout or fallback mechanism
- Added 8-second timeout but issue persisted

### Rollback Decision

**Decision:** Keep `dioxus-desktop/` as active frontend, archive `sandbox-ui/` (React)

**Rationale:**
- Dioxus has stable WebSocket implementation
- Terminal multi-browser/reload stability issues were fixable in Dioxus
- React implementation had architectural issues (state duplication, complex event handling)
- Development velocity higher with proven Dioxus codebase

### Fixes Since Rollback

- WebSocket stabilization: Replaced direct signal mutation with queued event processing
- Terminal connection reliability: Added watchdog timeout, improved event sequencing
- Window drag behavior: Moved to pointer lifecycle events (pointerdown/move/up)

---

*Last updated: 2026-02-06*
*Status: Rolled back to Dioxus, React archived*
*Commits: ~50 over 3 days*

---

# ChoirOS Progress - 2026-02-06 (Supervision Cutover Complete)

## Summary

**Supervision Cutover COMPLETE** - Successfully migrated from ActorManager-based architecture to ractor supervision tree. Removed ActorManager anti-patterns (DashMap, Mutex), all validation gates passing. Ready for multiagent rollout (ResearcherActor, DocsUpdaterActor, VerifierAgent, WatcherActors). See `docs/architecture/supervision-cutover-handoff.md` for full details.

## Major Achievements

**Supervision Tree Foundation:**
- Migrated to ractor supervision tree pattern
- Removed ActorManager central coordinator (anti-pattern)
- Direct actor-to-actor communication via ractor
- Event-driven architecture with EventStore as source of truth

**Validation Gates Passing:**
- Actor startup and shutdown sequences
- Message passing between actors
- Event persistence to SQLite
- WebSocket connectivity
- Desktop and window management
- Terminal operations
- Chat functionality

**Multiagent Ready:**
- Foundation laid for service actors
- VerifierAgent (pipelining, sandbox isolation)
- FixerActor (hotfix strategy, E2E reconciliation)
- ResearcherActor (web search, LLM inference)
- DocsUpdaterActor (in-memory index, system queries)
- WatcherActors (file system monitoring)

### Next Priority: Multiagent Rollout

**Phase 1: Service Actors**
- ResearcherActor - Web search and LLM inference
- DocsUpdaterActor - In-memory documentation index
- WatcherActors - File system change monitoring

**Phase 2: Verification & Pipelining**
- VerifierAgent - Sandbox isolation for code execution
- FixerActor - Hotfix strategy and E2E reconciliation

**Phase 3: Advanced Features**
- Multi-agent coordination patterns
- Supervisor message protocol
- Restart strategies (one_for_one, simple_one_for_one)

## Architecture Status

```
✅ dioxus-desktop/    - Active frontend (Dioxus 0.7)
✅ Supervision tree   - COMPLETE (ractor-based)
✅ Multiagent rollout - NEXT PHASE (per design doc)
```

**Reference:** `docs/architecture/supervision-cutover-handoff.md`

---

*Last updated: 2026-02-06*
*Status: Supervision cutover complete, ready for multiagent rollout*

---

# ChoirOS Progress - 2026-02-01

## Summary

**Major Day: Docs Cleanup, Coherence Fixes, Automatic Computer Architecture Defined** - Archived 9 outdated docs, fixed 18 coherence issues across core documents, created lean architecture doc, and handed off to event bus implementation. 27 commits today.

## Today's Commits (27 total)

**Recent (Last 3 hours):**
- `2472392` - handoff to event bus implementation
- `2472392` - docs: major cleanup, coherence fixes, and automatic computer architecture
- `9fe306c` - docs: add multi-agent vision and upgrade notes
- `471732b` - actorcode dashboard progress
- `473ca07` - actorcode progress

**Earlier Today:**
- `2084209` - feat: Chat App Core Functionality
- `bd9330f` - feat: add actorcode orchestration suite
- Plus 21 more: research system, clippy fixes, OpenCode Kimi provider fix, handoff docs, etc.

## What Was Accomplished Today

### ✅ Docs Cleanup (9 docs archived/deleted)
**Archived:**
- DEPLOYMENT_REVIEW_2026-01-31.md
- DEPLOYMENT_STRATEGIES.md  
- actorcode_architecture.md

**Deleted:**
- AUTOMATED_WORKFLOW.md
- DESKTOP_API_BUG.md
- PHASE5_MARKMARKDOWN_TESTS.md
- choirOS_AUTH_ANALYSIS_PROMPT.md
- feature-markdown-chat-logs.md
- research-opencode-codepaths.md

### ✅ Coherence Fixes (18 issues resolved)
**Critical fixes:**
- Removed Sprites.dev references (never implemented)
- Fixed actor list (removed WriterActor/BamlActor/ToolExecutor, added ChatAgent)
- Marked hypervisor as stub implementation
- Fixed test counts (18 → 171+)
- Updated dev-browser → agent-browser
- Marked Docker as pending NixOS research
- Marked CI/CD as planned (not implemented)
- Fixed port numbers (:5173 → :3000)
- Fixed database tech (SQLite → libSQL)
- Fixed API contracts
- Fixed BAML paths (sandbox/baml/ → baml_src/)
- Added handoffs to doc taxonomy
- Clarified actorcode dashboard separation
- Marked vision actors as planned
- Added OpenProse disclaimer
- Documented missing dependencies
- Fixed E2E test paths
- Rewrote AGENTS.md with task concurrency rules

### ✅ New Documentation
- `AUTOMATIC_COMPUTER_ARCHITECTURE.md` - Lean architecture doc (contrast with OpenAI's blocking UX)
- `dev-blog/2026-02-01-why-agents-need-actors.md` - Actor model argument
- `handoffs/2026-02-01-docs-upgrade-runbook.md` - 19 actionable fixes
- `handoffs/2026-02-01-event-bus-implementation.md` - Ready for next session

### ✅ New Skills & Tools
- **system-monitor** - ASCII actor network visualization
- **actorcode dashboard** - Multi-view web dashboard (list/network/timeline/hierarchy)
- **Streaming LLM summaries** - Real-time summary generation
- **NixOS research supervisor** - 5 workers + merge/critique/report pipeline
- **Docs upgrade supervisor** - 18 parallel workers for coherence fixes

### ✅ NixOS Research Complete
- 3/5 initial workers succeeded
- Merge → Web Conqitque → Final Report all completed
- Comprehensive research docs in `docs/research/nixos-research-2026-02-01/`

## What's Working

### Backend (sandbox) ✅
- **Server:** Running on localhost:8080
- **Database:** SQLite via sqlx with event sourcing
- **Actors:** EventStoreActor, ChatActor, DesktopActor, ActorManager, ChatAgent
- **API Endpoints:** Health, chat, desktop, websocket
- **WebSocket:** Connection works and stays alive
- **Chat processing:** Messages reach ChatAgent and AI responses return

### Frontend (sandbox-ui) ✅
- **Framework:** Dioxus 0.7 (WASM)
- **Desktop UI:** dock, floating windows, prompt bar
- **Chat UI:** modern bubbles, typing indicator, input affordances
- **Icon open:** chat opens from desktop icon (double-click)
- **WebSocket status:** shows connected

### Actorcode Orchestration ✅
- **Research system:** Non-blocking task launcher with findings database
- **Dashboard:** Multi-view with streaming summaries
- **Supervisors:** Can spawn parallel workers (docs upgrade: 18 workers)
- **Artifact persistence:** Workers write to JSONL logs

## Current Status

### ✅ Completed Today
- Major docs cleanup (9 docs archived/deleted)
- 18 coherence fixes across core documents
- Automatic computer architecture defined
- NixOS research completed
- System monitor skill
- Multi-view dashboard with streaming
- Task concurrency rules documented

### 📋 Next Steps (from handoff)
1. **Event Bus Implementation** - Build pub/sub system for async workers
2. **Worker Integration** - Make workers emit events
3. **Dashboard WebSocket** - Real-time event streaming
4. **Prompt Bar** - Shell-like interface for spawning workers

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     USER INTERFACE                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Prompt Bar  │  │  App Windows│  │   Dashboard         │  │
│  │ (shell)     │  │  (tmux)     │  │   (observability)   │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
└─────────┼────────────────┼────────────────────┼─────────────┘
          │                │                    │
          ▼                ▼                    ▼
┌─────────────────────────────────────────────────────────────┐
│                    EVENT BUS (Pub/Sub) ←── Next Session      │
└─────────────────────────────────────────────────────────────┘
```

## Key Insight: Anti-Chatbot

**OpenAI Data Agent:** "Worked for 6m 1s" → user blocked, staring at spinner
**ChoirOS Automatic Computer:** User spawns worker, continues working, observes via dashboard

The difference: Infrastructure vs Participant. We build the former.

## Documentation

**Authoritative:**
- `README.md` - Quick start
- `docs/AUTOMATIC_COMPUTER_ARCHITECTURE.md` - Core architecture
- `docs/ARCHITECTURE_SPECIFICATION.md` - Detailed spec (now coherent)
- `docs/DESKTOP_ARCHITECTURE_DESIGN.md` - Desktop design
- `AGENTS.md` - Development guide with concurrency rules

**Handoffs:**
- `docs/handoffs/2026-02-01-event-bus-implementation.md` - Ready to implement

**Research:**
- `docs/research/nixos-research-2026-02-01/` - NixOS deployment research

---

*Last updated: 2026-02-01 19:35*  
*Status: Major docs cleanup complete, architecture defined, ready for event bus*  
*Commits today: 27*

---

## 2026-02-09 Progress Update (Loop Simplification + Live Eval)

### Completed
- Simplified chat orchestration to a single autonomous loop (removed separate synthesis pass).
- Preserved non-blocking delegated work pattern (background research + follow-up signaling).
- Verified websocket chat integration suite still passes:
  - `cargo test -p sandbox --test websocket_chat_test -- --nocapture`
  - result: `20 passed, 0 failed`
- Re-ran live Superbowl no-hint matrix:
  - `cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture`
  - result summary:
    - `executed=15`
    - `strict_passes=8`
    - `non_blocking=true`
    - `signal_to_answer=true`
    - `polluted_count=0`
    - `search_then_bash=false`

### What We Learned
- The remaining issue is no longer basic async flow.
- Biggest gap is quality/continuation policy:
  - some provider/model combinations return noisy search-derived outputs,
  - autonomous `search -> terminal` escalation is still not discovered reliably.

### Immediate Next Steps
1. Unify `chat`, `terminal`, and `researcher` on one shared loop harness abstraction.
2. Add explicit continuation/escalation rules when search evidence is insufficient.
3. Add provider quality filtering/ranking before final answer emission.

## 2026-02-09 Policy + Control Plane Update

### Implemented
- Added capability escalation architecture doc:
  - `/Users/wiz/choiros-rs/docs/architecture/capability-escalation-policy.md`
- Implemented researcher objective status contract in runtime:
  - `objective_status`
  - `completion_reason`
  - `recommended_next_capability`
  - `recommended_next_objective`
- Implemented supervisor policy hook for `research -> terminal` escalation.

### Verification
- `cargo check -p sandbox` passed
- `cargo test -p sandbox --test websocket_chat_test -- --nocapture` passed (`20/20`)
- `BAML_LOG=ERROR CHOIR_SUPERBOWL_MATRIX_MODELS='KimiK25' CHOIR_SUPERBOWL_MATRIX_PROVIDERS='tavily' cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture` passed (`1/1`)
