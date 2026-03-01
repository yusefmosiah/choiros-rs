# Supervision Cutover Checklist (Safe Sequence)

Purpose: execute the minimum safe refactor sequence required before multiagent rollout.

This checklist reconciles:
- `docs/architecture/supervision-implementation-plan.md` (broad, multi-week migration)
- `docs/design/2026-02-06-multiagent-architecture-design.md` (future-state architecture)

Cutover priority is safety and operability over architecture ambition.
Defer non-blocking ambitions (watcher trees, service pools, hierarchical event buses, advanced load-balancing) until this checklist is complete.

## Global Invariants (must hold during cutover)

- No user-facing API contract regressions for chat, terminal, desktop, or viewer endpoints.
- No panic/todo/unimplemented in request-serving supervision paths.
- For any migrated domain, actor creation must be supervisor-owned (no direct unsupervised `Actor::spawn` path in runtime request flow).
- EventStore remains available and event schema compatibility is preserved.
- Each step lands behind green gates before proceeding; no stacked risky steps.

## Cutover Execution Status (2026-02-06)

- Step 1: complete (test matrix aligned for supervision-only runtime path)
- Step 2: complete (terminal request path routed through supervisors only)
- Step 3: complete (chat and chat-agent get/create paths implemented via `ChatSupervisor`)
- Step 4: complete (API handlers no longer call `actor_manager` runtime APIs)
- Step 5: complete (legacy `ActorManager` removed from runtime; all gates passing)

## Step 1 - Stabilize test/feature matrix

**Objective**
- Establish a reliable baseline matrix proving parity for `supervision_refactor` and defining required gates for each subsequent step.

**Files to change**
- `docs/architecture/supervision-cutover-checklist.md` (update matrix section as work progresses)
- `sandbox/tests/supervision_test.rs`
- `sandbox/tests/chat_api_test.rs`
- `sandbox/tests/websocket_chat_test.rs`
- `sandbox/tests/terminal_ws_smoketest.rs`
- `sandbox/tests/desktop_api_test.rs`

**Concrete tasks**
- Define a minimal gate matrix: compile, core API tests, websocket smoke, supervision integration.
- Add/adjust tests so they run with `--features supervision_refactor` (no ignored critical tests).
- Ensure flaky tests are fixed or quarantined with explicit rationale and issue link.
- Record baseline pass/fail in PR description and handoff doc.

**Pass/fail gates**
- `cargo check -p sandbox --features supervision_refactor`
  - Expected: exits `0`.
- `cargo test -p sandbox --features supervision_refactor --test supervision_test`
  - Expected: exits `0`.
- `cargo test -p sandbox --features supervision_refactor --test chat_api_test --test websocket_chat_test --test terminal_ws_smoketest --test desktop_api_test`
  - Expected: exits `0`.

**Rollback strategy**
- Revert only new/changed tests that introduced instability.
- Keep baseline commit tagged as `cutover-step1-green` before moving to Step 2.

## Step 2 - Single terminal runtime path (supervised only)

**Objective**
- Ensure terminal runtime has exactly one production path: supervised. Remove unsupervised fallback spawning.

**Files to change**
- `sandbox/src/supervisor/mod.rs`
- `sandbox/src/supervisor/session.rs`
- `sandbox/src/supervisor/terminal.rs`
- `sandbox/src/api/terminal.rs`
- `sandbox/src/actor_manager.rs` (intermediate shim only if still present)

**Concrete tasks**
- Remove direct fallback `TerminalActor` spawns from `ApplicationSupervisorMsg::GetOrCreateTerminal` flow.
- Make `SessionSupervisor -> TerminalSupervisor` path return a real usable actor reference (no placeholder error path).
- Eliminate retry logic that depends on `ActorManager` stale registry cleanup for terminal startup.
- Prefer correctness-first supervised implementation; defer terminal factory optimization if it risks rollout.

**Pass/fail gates**
- `rg "falling back to direct TerminalActor spawn|fallback TerminalActor" sandbox/src/supervisor/mod.rs`
  - Expected: no matches.
- `cargo test -p sandbox --features supervision_refactor --test terminal_ws_smoketest`
  - Expected: exits `0`, terminal websocket can start/attach without fallback.
- `cargo check -p sandbox --features supervision_refactor`
  - Expected: exits `0`.

**Rollback strategy**
- Roll back to `cutover-step1-green` artifact/commit if terminal supervision path is unstable.
- Do not reintroduce dual runtime paths; rollback should be full binary rollback, not runtime fallback toggles.

## Step 3 - Implement real chat supervisor request paths

**Objective**
- Replace phase placeholders with real `GetOrCreateChat` and `GetOrCreateChatAgent` supervised flows.

**Files to change**
- `sandbox/src/supervisor/mod.rs`
- `sandbox/src/supervisor/session.rs`
- `sandbox/src/supervisor/chat.rs` (new)
- `sandbox/src/api/chat.rs`
- `sandbox/src/api/websocket_chat.rs`

**Concrete tasks**
- Add `ChatSupervisor` with deterministic get/create semantics for chat actors and chat agents.
- Route `ApplicationSupervisorMsg::{GetOrCreateChat, GetOrCreateChatAgent}` through `SessionSupervisor` to `ChatSupervisor`.
- Remove panic placeholders and phase-1 temporary behavior.
- Preserve current event persistence behavior for user/assistant/tool events.

**Pass/fail gates**
- `rg "Not implemented in Phase 1|panic!\(\"Not implemented in Phase 1\"" sandbox/src/supervisor`
  - Expected: no matches.
- `cargo test -p sandbox --features supervision_refactor --test chat_api_test --test websocket_chat_test`
  - Expected: exits `0`.
- `cargo check -p sandbox --features supervision_refactor`
  - Expected: exits `0`.

**Rollback strategy**
- Revert only chat-supervision commits and return to `cutover-step2-green`.
- Keep terminal cutover intact; do not undo Step 2 unless required for global stability.

## Step 4 - Move API layer off ActorManager to supervisors

**Objective**
- Make API handlers depend on supervisor-oriented app state, not `ActorManager`.

**Files to change**
- `sandbox/src/main.rs`
- `sandbox/src/api/mod.rs`
- `sandbox/src/api/chat.rs`
- `sandbox/src/api/terminal.rs`
- `sandbox/src/api/desktop.rs`
- `sandbox/src/api/websocket.rs`
- `sandbox/src/api/websocket_chat.rs`
- `sandbox/src/api/viewer.rs`
- `sandbox/src/api/user.rs`

**Concrete tasks**
- Introduce/expand app state to carry supervisor refs + `event_store` directly.
- Replace API calls that access `state.app_state.actor_manager.*` with supervisor RPC calls.
- Keep endpoint request/response payloads unchanged.
- Remove API-level dependency on `ActorManager::remove_terminal` cleanup semantics.

**Pass/fail gates**
- `rg "actor_manager" sandbox/src/api -g '*.rs'`
  - Expected: no runtime usage matches.
- `cargo test -p sandbox --features supervision_refactor --test chat_api_test --test websocket_chat_test --test terminal_ws_smoketest --test desktop_api_test --test viewer_api_test`
  - Expected: exits `0`.
- `cargo check -p sandbox --features supervision_refactor`
  - Expected: exits `0`.

**Rollback strategy**
- Revert API migration commits to `cutover-step3-green` if handler parity breaks.
- Keep supervisor internals from Steps 2-3 intact while fixing API wiring.

## Step 5 - Remove ActorManager runtime usage

**Objective**
- Remove legacy ActorManager from runtime path and leave supervision as the only execution model.

**Files to change**
- `sandbox/src/actor_manager.rs` (delete or convert to non-runtime compatibility stub)
- `sandbox/src/main.rs`
- `sandbox/src/lib.rs` (if exports reference `actor_manager`)
- `sandbox/Cargo.toml` (remove now-unused runtime deps, e.g., DashMap if unused)
- `docs/architecture/supervision-implementation-plan.md` (mark legacy sections as completed/superseded)

**Concrete tasks**
- Remove runtime construction/usage of `ActorManager`.
- Remove direct actor registry logic that duplicates ractor registry/supervisors.
- Collapse startup to supervision-first path (no legacy branch).
- Ensure tests and docs reflect supervision-only runtime assumptions.

**Pass/fail gates**
- `rg "ActorManager|dashmap::DashMap|terminal_create_lock" sandbox/src -g '*.rs'`
  - Expected: no runtime path matches.
- `cargo clippy -p sandbox --features supervision_refactor -- -D warnings`
  - Expected: exits `0`.
- `cargo test -p sandbox --features supervision_refactor`
  - Expected: exits `0`.

**Rollback strategy**
- Roll back to `cutover-step4-green` if full-suite stability regresses.
- If rollback is required after deployment, roll back entire deploy artifact, not selective hot edits.

## Definition of Done (before multiagent rollout)

- All 5 steps merged with green gates and step tags (`cutover-step1-green` ... `cutover-step5-green`).
- No panic placeholders or unsupervised fallback in runtime request paths.
- API layer no longer depends on `ActorManager`.
- `ActorManager` is absent from runtime execution path.
- Supervision-only build/test results are reproducible on clean checkout.

## Handoff Checklist (final handoff doc must include)

- Exact commit SHA per step and corresponding gate outputs (command + pass/fail).
- Any deviations from this checklist and rationale.
- Remaining known risks and explicit non-goals deferred post-rollout.
- Rollback target (latest known-good tag/artifact) and trigger criteria.
- Operator notes: required feature flags/config state and smoke-test commands.
