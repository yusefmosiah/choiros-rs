# Handoff: Chat Tool Streaming + UI Architecture Reconciliation

## Session Metadata
- Created: 2026-02-05 14:44:56
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~2h

### Recent Commits (for context)
- 1af20ed multiturn chat
- 54aa600 Complete Axum migration cleanup across deps, tests, and docs
- b8fa802 Restore app interactivity via chat WS
- e2f23af Add terminal UI integration and E2E smoke
- 4ec9d58 terminal ui doc

## Handoff Chain
- **Continues from**: [2026-02-05-123043-axum-refactor.md](./2026-02-05-123043-axum-refactor.md)
  - Previous title: Axum Refactor In Sandbox
- **Supersedes**: None

## Current State Summary
Chat was upgraded to a working multi-turn/tool-calling flow across HTTP and WebSocket. The key fix was making tool events stream in real time to the UI rather than appearing only when final assistant text is ready. Tool call/result messages are now rendered as expandable sections in chat. Separately, research docs were reviewed and there are architecture mismatches that need reconciliation before UI implementation planning proceeds (especially around IndexedDB/localStorage recommendations vs actor/EventStore source-of-truth architecture).

## Codebase Understanding

### Architecture Overview
- Backend state authority is actor + EventStore (SQLite/libsql), not browser storage.
- Chat processing flow:
  1. User message is appended to EventStore.
  2. `ChatAgent` processes turn, logs tool call/result and assistant events.
  3. WebSocket streams incremental tool events while agent is still running.
- Frontend chat consumes stream chunks and renders system/tool messages as expandable UI blocks.

### Critical Files
| File | Purpose | Relevance |
|------|---------|-----------|
| `../sandbox/src/actors/chat_agent.rs` | Stateful chat agent, model switching, event logging, history load | Multi-turn correctness and tool execution |
| `../sandbox/src/api/websocket_chat.rs` | WebSocket chat protocol and live chunk emission | Real-time tool streaming fix |
| `../sandbox/src/api/chat.rs` | HTTP send/get message path and persistence behavior | Consistency with agent/event model |
| `../sandbox-ui/src/components.rs` | Chat UI rendering and stream handling | Expandable tool call/result sections |
| `ARCHITECTURE_SPECIFICATION.md` | Canonical architecture contract | Reconciliation baseline |
| `window-management-research.md` | New UI research for window mgmt | Mostly aligned, needs policy framing |
| `content-viewer-research.md` | New UI research for viewers | Contains storage guidance that conflicts |
| `theme-system-research.md` | New UI research for theming | Contains localStorage-first recommendation |
| `research-dioxus-architecture.md` | New UI structure/state guidance | Contains useful decomposition guidance, some drift |

### Key Patterns Discovered
- Keep UI optimistic state ephemeral; authoritative state must round-trip via backend actors/events.
- Stream intermediate events from EventStore to avoid artificial "batching" delays in UX.
- Use structured payload markers for tool messages in UI (`__tool_call__:` / `__tool_result__:`) so renderer can switch to rich components.

## Work Completed

### Tasks Finished
- [x] Refactored `ChatAgent` to keep persistent in-actor state for conversation history and model.
- [x] Added history bootstrap from EventStore at agent startup.
- [x] Unified persistence semantics so user/assistant/tool events are event-driven and recoverable.
- [x] Added live WebSocket streaming of `chat.tool_call` / `chat.tool_result` events while agent runs.
- [x] Implemented expandable tool call/result sections in chat UI.
- [x] Reviewed new research docs for architectural contradictions.

### Files Modified
| File | Changes | Rationale |
|------|---------|-----------|
| `../sandbox/src/actors/chat_agent.rs` | Moved mutable conversation/model/tool registry state into `ChatAgentState`; loaded prior messages from EventStore; synchronous-in-actor processing instead of clone-per-call pattern | Fix broken multi-turn behavior and make model switching persistent |
| `../sandbox/src/api/chat.rs` | Ensured HTTP path appends user event immediately, then triggers agent async | Maintain responsiveness and ordered event history |
| `../sandbox/src/api/websocket_chat.rs` | Added polling event streamer for incremental tool events with seq cursor + completion drain; removed delayed tool replay from final response path | Tool calls/results now appear in UI as they happen |
| `../sandbox-ui/src/components.rs` | Added tool payload tagging and expandable rendering via `<details>` sections for calls/results; styled system/tool message rows | Better tool transparency and usable debug UX |

### Decisions Made
| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Stream tool events from EventStore during processing | Emit only after `ProcessMessage` returns; direct callback channel from agent; EventStore polling | EventStore-based stream reuses existing architecture and preserves canonical ordering without deeper actor protocol changes |
| Keep backend source of truth for chat/tool history | Browser-only cache/state for tool flow | Aligns with architecture spec and recovery guarantees |
| Render tool events as structured expandable sections | Keep plain text bubbles; add separate tool panel | Expandable inline sections preserve conversation context and keep implementation simple |

## Architecture Review Findings
Findings are ordered by severity and reference exact doc lines.

1. **High:** Storage guidance conflicts with actor-owned state model.
- `content-viewer-research.md:170` recommends IndexedDB content persistence.
- `content-viewer-research.md:802` recommends IndexedDB playlist persistence.
- `theme-system-research.md:241` marks localStorage as recommended persistence path.
- This conflicts with `ARCHITECTURE_SPECIFICATION.md:44-45` (actor-owned state/UI projection) and `ARCHITECTURE_SPECIFICATION.md:998` (no localStorage caching for MVP).

2. **Medium:** Research doc has stale factual metadata about current UI implementation.
- `research-dioxus-architecture.md:6` claims Dioxus `0.5.7`, but workspace currently builds against `0.7.x`.
- `research-dioxus-architecture.md:50` says `components.rs` is empty; it now contains active chat UI and tool section rendering.

3. **Medium:** Some “comprehensive” docs listed in summary are not currently present as standalone files under `../`.
- Present at root: `../window-management-research.md`, `../content-viewer-research.md`, `../theme-system-research.md`, `../research-dioxus-architecture.md`.
- Not found as separate files in `../` during this review: drag/drop, mail/calendar, JS interop, file explorer research docs.
- Action: either add those docs or update the summary inventory to avoid planning off missing artifacts.

## Pending Work

## Immediate Next Steps
1. [x] Create a short architecture reconciliation note (ADR-style) that defines UI storage policy:
   - Authoritative domain state (windows/files/messages/tool history/themes if shared) stays in actors/EventStore.
   - Browser persistence limited to optional non-authoritative UX cache and feature flags.
2. [x] Update research docs with an explicit “ChoirOS Compatibility” section per document:
   - Keep techniques (e.g., Pointer Events, transforms, lazy loading).
   - Replace storage recommendations that violate backend source-of-truth.
3. [x] Build a UI implementation backlog from reconciled docs:
   - Window management refactor/decomposition.
   - Viewer framework shell (without IndexedDB migration).
   - Theme system via backend-backed preference endpoint first, optional client cache second.
4. [x] Chat UI follow-up:
   - Persist/restore tool sections from `GET /chat/{actor_id}/messages` payload mapping.
   - Improve incremental thinking/status chunk UX (optional).

### Blockers/Open Questions
- [x] Decide final policy for browser-side caches: allowed as optional non-authoritative cache/write-through optimization only.
- [ ] Confirm whether theme preference is user-global (backend profile) or sandbox-local actor state.
- [ ] Confirm where missing research docs are stored (if not `../`).

### Deferred Items
- Drag/resize implementation details from research were deferred pending reconciliation policy.
- Viewer/media integration deferred until storage and security posture is explicitly aligned.

## Context for Resuming Agent

## Important Context
The immediate priority is not implementing new UI features blindly from research docs; it is reconciling those plans with existing architecture guarantees. Specifically: do not migrate domain state to IndexedDB/localStorage as primary persistence. Keep actor/EventStore as source of truth. The chat stack now demonstrates the intended pattern: events are authoritative, UI streams and projects state in real time, and rich UX (expandable tool sections) can still be achieved without local-first divergence.

### Assumptions Made
- `ARCHITECTURE_SPECIFICATION.md` is still the canonical contract unless explicitly superseded.
- Missing research docs outside `../` may exist elsewhere or were not committed yet.
- Local browser persistence, if used, should be additive cache, never canonical state.

### Potential Gotchas
- Reintroducing duplicate tool events is easy if both “live event stream” and “post-response replay” are active; keep only one emission path.
- Chat UI tool sections now render from both WS and HTTP history mapping; keep payload prefix format (`__tool_call__:`/`__tool_result__:`) stable across paths.
- The repo has many unrelated modified files; avoid broad cleanup/reformat while doing focused UI reconciliation tasks.

## Environment State

### Tools/Services Used
- Rust toolchain (`cargo fmt`, `cargo check`, `cargo test`)
- Ractor actor runtime
- Dioxus frontend
- Session handoff scripts

### Active Processes
- None left running by this session.

### Environment Variables
- `AWS_BEARER_TOKEN_BEDROCK`
- `ZAI_API_KEY`

## Related Resources
- `../ARCHITECTURE_SPECIFICATION.md`
- `../window-management-research.md`
- `../content-viewer-research.md`
- `../theme-system-research.md`
- `../research-dioxus-architecture.md`
- `../design/2026-02-05-ui-storage-reconciliation.md`
- `../design/2026-02-05-ui-implementation-backlog.md`
- `../sandbox/src/api/websocket_chat.rs`
- `../sandbox/src/actors/chat_agent.rs`
- `../sandbox-ui/src/components.rs`

---

**Validation Reminder**: Run `python skills/session-handoff/scripts/validate_handoff.py handoffs/2026-02-05-144456-chat-tool-streaming-ui-next-steps.md`.
