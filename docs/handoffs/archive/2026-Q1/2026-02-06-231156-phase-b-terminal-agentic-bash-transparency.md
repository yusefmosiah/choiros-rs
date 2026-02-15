# Handoff: Phase B Terminal Agentic Bash Transparency

## Session Metadata
- Created: 2026-02-06 23:11:56
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~3.5 hours

### Recent Commits (for context)
  - 0e77f99 Document ChatAgent scope fixes
  - f67ee36 Document scoped roadmap progress
  - e53c1f9 Act on roadmap progress update
  - 0b7fc0b Document critical roadmap gaps
  - d9790c3 docs: archive React migration docs and remove sandbox-ui directory

## Handoff Chain

- **Continues from**: [2026-02-06-documentation-cleanup-and-progress-update.md](./2026-02-06-documentation-cleanup-and-progress-update.md)
  - Previous title: Documentation Cleanup and Progress Update
- **Supersedes**: None

## Current State Summary

Phase B control-plane implementation is active and now routes chat `bash` calls through supervisor delegation into `TerminalActor` with an internal agentic harness. Worker lifecycle events are persisted with scope + correlation IDs, websocket maps these events to `actor_call` chunks, and completion payloads now include transparency fields (`reasoning`, `executed_commands`, `steps`). A correctness fix landed so non-zero terminal exits emit `worker_failed` instead of `worker_complete`. Current backend tests for supervision/chat are green.

## Codebase Understanding

### Architecture Overview

- Chat boundary should remain a single `bash` tool contract for simplicity.
- Supervisor is control-plane coordinator (`DelegateTerminalTask`) and publishes canonical worker lifecycle events.
- Terminal domain now owns execution strategy (direct command vs multi-step planned execution), preserving transparency while keeping orchestration out of ChatAgent.
- EventStore polling in ChatAgent is currently the return channel for delegated task completion/failure.

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `sandbox/src/actors/chat_agent.rs` | Chat planning/tool execution and delegated task wait loop | `bash` now fully delegates through supervisor; no direct `bash` execution |
| `sandbox/src/supervisor/mod.rs` | Application control-plane orchestration | Added `DelegateTerminalTask` flow and worker event publication |
| `sandbox/src/actors/terminal.rs` | PTY runtime + new agentic terminal harness | Added `RunAgenticTask`, transparent step capture, failure semantics |
| `sandbox/src/api/websocket_chat.rs` | Chat websocket streaming | Maps worker lifecycle events to `actor_call` chunks |
| `sandbox/tests/supervision_test.rs` | Integration gate tests | Validates delegation, correlation continuity, and non-zero exit failure behavior |
| `roadmap_progress.md` | Execution tracking | Updated with Phase B implementation + review outcomes |
| `docs/architecture/roadmap-dependency-tree.md` | Dependency roadmap | Added execution snapshot for current implementation state |
| `docs/architecture/roadmap-critical-analysis.md` | Gap analysis | Added addendum reflecting newly closed gaps and remaining risks |

### Key Patterns Discovered

- Use `EventBus` + persisted `EventStore` events as cross-actor tracing backbone.
- Keep task lifecycle standardized: `worker.task.started/progress/completed/failed`.
- Preserve `session_id`/`thread_id` scope on emitted payloads to prevent chat-instance bleed.
- Prefer supervisor-managed delegation over actor-local shell subprocess execution.

## Work Completed

### Tasks Finished

- [x] Implemented async terminal delegation control-plane contract in supervisor/shared types/app state.
- [x] Routed chat `bash` calls through delegation path.
- [x] Added terminal-side agentic harness (`RunAgenticTask`) with transparency output.
- [x] Added websocket mapping for worker lifecycle events into `actor_call` stream chunks.
- [x] Fixed failure semantics for non-zero command exits (`worker_failed`).
- [x] Added regression test for non-zero exit behavior.
- [x] Updated roadmap/progress documents with current state and residual risks.

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| `sandbox/src/supervisor/chat.rs` | Threaded `application_supervisor` into ChatAgent args | Enable chat-to-supervisor delegation path |
| `sandbox/src/supervisor/mod.rs` | Added `DelegateTerminalTask` orchestration and worker event publishing; integrated terminal agent result payload | Implement Phase B control plane and task lifecycle |
| `sandbox/src/supervisor/session.rs` | Passed application supervisor into chat supervisor | Maintain delegation references across supervisor tree |
| `sandbox/src/api/websocket_chat.rs` | Emitted `actor_call` chunks for `worker_*` event types | Surface delegated worker lifecycle in chat stream |
| `sandbox/src/actors/chat_agent.rs` | Delegated `bash` tool calls; polled EventStore for delegated completion/failure | Remove direct `bash` execution and maintain async tool result behavior |
| `sandbox/src/actors/terminal.rs` | Added `RunAgenticTask`, step/result transparency fields, command marker extraction, failure status shaping | Move execution intelligence to terminal domain while preserving traceability |
| `sandbox/src/app_state.rs` | Added `delegate_terminal_task(...)` API | Provide app-level entrypoint to supervisor delegation |
| `sandbox/tests/supervision_test.rs` | Added delegation acceptance, gate trace continuity, and non-zero exit failure tests | Enforce behavior and regression safety |
| `shared-types/src/lib.rs` | Added delegated task/result structs and worker topic constants | Standardize control-plane contracts |
| `roadmap_progress.md` | Added checklist/progress/review updates | Keep execution status current |
| `docs/architecture/roadmap-dependency-tree.md` | Added 2026-02-07 execution snapshot | Reflect real implementation state |
| `docs/architecture/roadmap-critical-analysis.md` | Added update addendum | Close drift between analysis and implementation |
| `sandbox/src/supervisor/desktop.rs` | Formatting-only change | `cargo fmt` normalization |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Keep chat boundary as single `bash` tool | Expose objective-specific terminal tool vs keep `bash` | Preserves model simplicity and backwards-compatible tool interface |
| Move multi-step execution intelligence into terminal domain | Keep orchestration in supervisor/chat vs terminal-owned harness | Aligns with multiagent separation of concerns |
| Preserve transparency via payload fields (`reasoning`, `executed_commands`, `steps`) | Opaque terminal summary only | Enables future UI “LLM transparency” timeline |
| Mark non-zero command exits as worker failures | Treat all terminal runs as completed-with-output | Keeps tool contract semantics clear and error-handling deterministic |

## Pending Work

### Immediate Next Steps

1. Render dedicated UI timeline cards for worker/terminal transparency fields (`reasoning`, `steps`, `executed_commands`) in chat stream.
2. Add step-level progress event emission from terminal harness (plan/step/synthesis) instead of completion-only transparency.
3. Unify terminal-agent model/client selection with ChatAgent model policy (avoid Bedrock-only hardcoding).

### Blockers/Open Questions

- [ ] Should terminal harness always run planner for natural-language objectives, or should chat continue sending explicit shell commands only?
- [ ] Do we want `DelegatedTaskResult`/shared-types to formally include transparency fields (typed), instead of JSON payload-only transport?

### Deferred Items

- Full frontend actor-call timeline rendering: deferred until backend event shape stabilized.
- Full non-chat scope propagation and enforcement (desktop/terminal domains): deferred to upcoming Phase F hardening pass.

## Immediate Next Steps

1. Implement frontend rendering for terminal transparency payload (`reasoning`, `steps`, `executed_commands`) in chat timeline.
2. Emit step-level terminal progress events (plan/step/synthesis) rather than completion-only payload details.
3. Align terminal-agent model selection/config with ChatAgent model policy.

## Context for Resuming Agent

### Important Context

- Current behavior is intentionally hybrid:
  - Chat still invokes `bash`.
  - Supervisor delegates to terminal.
  - Terminal may execute directly (command-like input) or via planner loop.
- Non-zero exit status propagation was a subtle bug and is now fixed; keep this invariant protected.
- `supervision_test` includes a deterministic non-zero test command (`false && echo should_not_run`) to avoid LLM plan nondeterminism.
- Existing manual UX pain point remains: timeline visualization is not yet first-class in frontend despite backend transparency payloads being available.

## Important Context

- The `bash` tool is intentionally the only chat-facing execution API.
- Multi-step execution is now terminal-domain logic (`TerminalMsg::RunAgenticTask`), not chat/supervisor command choreography.
- Non-zero exit statuses now propagate as `worker_failed` and are covered by regression tests.
- Current remaining gap is UI rendering/interaction depth, not backend delegation correctness.

### Assumptions Made

- Backend test environment can access current BAML runtime; planner-dependent tests may still be subject to model/runtime availability if expanded.
- Existing uncommitted workspace modifications are intentional and should not be reverted.

### Potential Gotchas

- `TerminalActor` agentic path currently builds its own Bedrock client registry and does not share chat model switching config.
- Planner branches can be nondeterministic; use command-pattern test cases for deterministic behavior tests.
- Worker event `event_type` values are `worker_spawned/progress/complete/failed`, while topics are `worker.task.*`; UI and tests should not conflate them.

## Environment State

### Tools/Services Used

- Rust toolchain with `cargo fmt`, `cargo check`, `cargo test`.
- Existing BAML client integration (`sandbox/src/baml_client` generated bindings).

### Active Processes

- No long-running local services intentionally left running by this session.

### Environment Variables

- `AWS_BEARER_TOKEN_BEDROCK` (used by Bedrock client path, if present)
- `ZAI_API_KEY` (used by GLM path in chat agent)

## Related Resources

- `/Users/wiz/choiros-rs/roadmap_progress.md`
- `/Users/wiz/choiros-rs/docs/architecture/roadmap-dependency-tree.md`
- `/Users/wiz/choiros-rs/docs/architecture/roadmap-critical-analysis.md`
- `/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs`
- `/Users/wiz/choiros-rs/sandbox/tests/supervision_test.rs`

---

**Security Reminder**: Validated with `validate_handoff.py` in this session.
