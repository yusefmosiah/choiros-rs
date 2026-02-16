# Writeractor + Supervision Normalization Runbook (2026-02-16)

## Narrative Summary (1-minute read)

This runbook defines the refactor that unifies Writer and RunWriter responsibilities under one
`WriterActor` control plane while preserving durable run-document version history.

The target architecture has three concrete outcomes:

1. Conductor actor lifecycle is normalized into the supervision tree (no direct app-state spawn).
2. Writer message passing is normalized to typed envelopes with correlation metadata across
   Conductor, Researcher, and Writer self-messages.
3. Writer orchestration moves to `AgentHarness` semantics so writer can both revise documents and
   direct workers (Researcher/Terminal) using the same control-flow model as other workers.

Because this is a high-risk surface, execution is staged. This runbook includes the full plan and
marks Phase 1 as immediately implementable in this cycle.

## What Changed

### Scope and Outcomes

- Chosen refactor name: **Writeractor** (unified Writer + RunWriter control path).
- Chosen execution strategy: phased migration with compatibility rails, not big-bang rewrite.
- Chosen runtime normalization: Conductor is created through supervisors, consistent with existing
  Desktop/Terminal/Researcher/Writer paths.

### Target State

- `ApplicationSupervisor -> SessionSupervisor -> {ConductorSupervisor, TerminalSupervisor,
  ResearcherSupervisor, WriterSupervisor, DesktopSupervisor}`
- `WriterActor` owns:
  - inbound message queueing
  - delegation decisions via `AgentHarness`
  - canonical revision synthesis and section-state signaling
- Durable document history remains explicit and append-only:
  - version snapshots
  - overlays and patch provenance
  - retrieval by version id and revision
- Writer ingress contract becomes one typed envelope for all sources (user/conductor/researcher/
  terminal/writer), including correlation metadata.

### Phased Plan

#### Phase 1: Control-Path Normalization (implement now)

- Add supervised conductor lifecycle APIs and remove direct conductor spawn in `AppState`.
- Add typed writer inbound envelope and migrate all `EnqueueInbound` callsites.
- Keep existing Writer synthesis/delegation behavior stable (no model-contract churn in this phase).

#### Phase 2: Writer Harness Integration

- Introduce a writer-specific `WorkerPort` adapter.
- Move writer delegation planning/execution from ad-hoc LLM calls to `AgentHarness::run`.
- Preserve writer authority over revision commits by routing final mutations through run-document
  patch/version APIs.

#### Phase 3: Writeractor Structural Unification

- Collapse duplicated state/ownership boundaries between Writer and RunWriter while preserving
  version durability interfaces.
- Reconcile naming and APIs so `WriterActor` is the canonical app-facing actor; internal storage
  components become implementation details.

#### Phase 4: Persistence and Retrieval Hardening

- Persist document versions/overlays to server durable storage on every mutation.
- Add cold-start restoration and retrieval APIs for offloaded/archived versions.
- Add compaction policy: preserve full lineage while allowing tiered storage for old snapshots.

### Message Semantics Contract (Writer Inbound)

Each inbound writer message must include:

- message identity: `message_id`, `correlation_id`
- scope: `run_id`, `section_id`, `source`, `kind`
- payload: `content`, optional `prompt_diff`
- state references: optional `base_version_id`, `overlay_id`
- routing metadata: optional `session_id`, `thread_id`, `call_id`, `origin_actor`

### Success Criteria

- Conductor spawn path no longer uses direct `Actor::spawn` in `AppState`.
- All writer enqueue paths use one typed envelope.
- `cargo check -p sandbox` passes after migration.
- Existing writer/researcher/conductor test surfaces still pass.

## What To Do Next

1. Implement Phase 1 code changes in `sandbox/src/app_state.rs`,
   `sandbox/src/supervisor/*.rs`, `sandbox/src/actors/writer/mod.rs`, and Writer callsites.
2. Run `cargo fmt`, `cargo check -p sandbox`, and targeted tests for supervisor/writer/conductor.
3. Open Phase 2 branch for harness integration with explicit test-first checklist:
   - writer delegation contract tests
   - websocket actor-call streaming assertions
   - regression tests for writer queue dedupe and revision sequencing
4. Add persistence ADR for version/offload retrieval before Phase 4 implementation.
