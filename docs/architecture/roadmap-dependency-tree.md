# ChoirOS Roadmap (Linear Execution Checklist)

Date: 2026-02-08
Status: Authoritative execution order

## Why Linear

We are intentionally moving from a dependency tree to a linear checklist.

Reason:
- Parallel feature development is creating architectural drift and unclear ownership.
- We need one active milestone at a time, with explicit gates.
- Work can still have small supporting tasks, but only one primary roadmap phase is in progress.

## Operating Rules

- Single active roadmap phase at a time.
- No new feature branch before current phase gate passes.
- Bug fixes are allowed at any time, but do not advance roadmap phase state.
- Documentation must be updated at each phase gate.

## Linear Checklist

### Phase 0: Directives App Lock

Goal:
- Establish Directives as the first-class operator view (app/window, not always-on panel).

Checklist:
- [ ] Define `DirectiveForest` state model.
- [ ] Define directive event schema (`directive.created|updated|blocked|completed`).
- [ ] Define WS stream contract for directive updates.
- [ ] Define Directives app open/focus behavior across mobile/desktop.

Gate:
- Directives data + event contracts are documented and approved.

### Phase 1: Capability Contract Lock

Goal:
- Lock shared actor-capability contracts before adding more actors.

Checklist:
- [ ] Finalize `CapabilityInput`, `CapabilityOutput`, `CapabilityEvent` schema.
- [ ] Finalize actor-call envelope (`task_id`, `correlation_id`, scope keys, status).
- [ ] Define mandatory event metadata fields.

Gate:
- Contract types compile and docs are signed off.

### Phase 2: StateIndex Baseline

Goal:
- Build compressed state/query plane for directives and context summaries.

Checklist:
- [ ] Implement StateIndexActor skeleton.
- [ ] Add compressed snapshot model with directives summary.
- [ ] Add query API (`GetLatestSnapshot`, `GetDirectiveTree`, `GetAlerts`).

Gate:
- Snapshot replay works and directive tree queries are stable.

### Phase 3: Identity and Scope Enforcement

Goal:
- Enforce strict scope boundaries before broad feature expansion.

Checklist:
- [ ] Require and validate scope keys across API and actor boundaries.
- [ ] Add isolation tests for cross-session/thread/user leakage.
- [ ] Ensure scope propagation in event and retrieval paths.

Gate:
- Isolation tests pass; no known cross-scope leaks.

### Phase 4: PromptBar Baseline

Goal:
- PromptBar becomes orchestration entrypoint.

Checklist:
- [ ] PromptBar routes to actors (not tools).
- [ ] PromptBar memo output format defined.
- [ ] PromptBar can open/focus Directives app and create directives.

Gate:
- PromptBar orchestration flows work end-to-end.

### Phase 5: Researcher Actor v1

Goal:
- Deliver first networked capability actor with observable lifecycle.

Checklist:
- [ ] Implement ResearcherActor capability contract.
- [ ] Add `web_search` abstraction in chat routing.
- [ ] Emit structured lifecycle events with citations.

Gate:
- Research flow is observable and reproducible in event log.

### Phase 6: Git Actor v1

Goal:
- Deliver first local capability actor with deterministic operations.

Checklist:
- [ ] Implement typed git operations.
- [ ] Add full event trail for git actions.
- [ ] Add baseline safety constraints for write operations.

Gate:
- Git workflows pass integration tests with full traceability.

### Phase 7: Chat Delegation Baseline

Goal:
- Chat routes capabilities cleanly without direct execution leakage.

Checklist:
- [ ] Chat `bash` interface delegates only to TerminalActor path.
- [ ] Remove/deny remaining direct shell execution paths outside TerminalActor.
- [ ] Verify chat tool transparency in event stream.

Gate:
- All chat shell actions show delegated worker traces only.

### Phase 8: Context Broker Baseline

Goal:
- Add layered retrieval grounded in scoped state.

Checklist:
- [ ] Implement brief context and handle expansion API.
- [ ] Integrate with StateIndex snapshots.
- [ ] Add relevance and scope-boundary tests.

Gate:
- Context retrieval improves continuity without violating scope isolation.

### Phase 9: Deterministic Safety Layer (Policy Deferred from Hot Path)

Goal:
- Add safety hardening after baseline capability flow is stable.

Checklist:
- [ ] Add deterministic local policy checks as mandatory boundaries.
- [ ] Add deterministic PolicyActor for high-risk escalations only.
- [ ] Add ModelPolicyWorker for deterministic model-routing support.
- [ ] Keep normal actor-to-actor calls off policy/supervisor hot path.

Gate:
- High-risk escalation paths enforced; low/medium paths remain fast and deterministic.

### Phase 10: PDF App Implementation Guide (Deferred)

Goal:
- Complete guide-first milestone before any PDF app implementation.

Checklist:
- [ ] Finalize `docs/architecture/pdf-app-implementation-guide.md` scope and API notes.
- [ ] Define render/extract test plan and deferred items.

Gate:
- Guide accepted; PDF app remains deferred.

### Phase 11: Persistence + Hypervisor Foundation

Goal:
- Land durable filesystem and hypervisor lifecycle foundations.

Checklist:
- [ ] SandboxFS persistence and snapshot/rehydrate baseline.
- [ ] Hypervisor attach/detach/restore lifecycle baseline.

Gate:
- Deterministic restore tests pass.

### Phase 12: Self-Hosting Introspection

Goal:
- Enable safe self-modification workflows after foundations are stable.

Checklist:
- [ ] Prompt visibility/editing with audit trail.
- [ ] Code introspection baseline.
- [ ] Headless test sandbox for safe validation.
- [ ] Rollback-tested safe modification loop.

Gate:
- End-to-end self-modification demo passes with explicit safeguards.

## Deferred / Not On Current Critical Path

- Agentic policy actors (beyond advisory mode) are post-deployment hardening.
- Broad app expansion beyond Directives/PromptBar/Researcher/Git is deferred.
- PDF app implementation is deferred until guide gate passes.

## References

- `docs/architecture/directives-execution-checklist.md`
- `docs/architecture/pdf-app-implementation-guide.md`
- `docs/architecture/model-provider-agnostic-runbook.md`
