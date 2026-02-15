# Documentation Consolidation Runbook

Date: 2026-02-14
Purpose: Clean up, consolidate, and organize ChoirOS documentation

---

## Narrative Summary (1-minute read)

**Problem:** 100+ markdown files accumulated over 2 weeks of intense development. Many describe completed work, superseded decisions, or deleted React code.

**Solution:** Delete 15+ obsolete files, archive 50+ completed handoffs, consolidate duplicates, and establish a clear doc lifecycle.

**Result:** ~30 current, authoritative docs organized by purpose. NARRATIVE_INDEX.md remains the human entry point.

---

## Current Code State vs Documentation

### What Actually Exists in Code (2026-02-14)

| Component | Status | Notes |
|-----------|--------|-------|
| **Actors** | Working | EventStore, Desktop, Terminal, Researcher, Conductor, Writer, RunWriter, EventBus, EventRelay |
| **Supervision Tree** | Working | ApplicationSupervisor -> SessionSupervisor -> per-type supervisors |
| **API Endpoints** | Working | Desktop, Terminal, Files, Writer, Conductor, Logs, Viewer |
| **Frontend (Dioxus)** | Working | DesktopShell, WorkspaceCanvas, PromptBar, WebSocket runtime |
| **Model Providers** | Working | AWS Bedrock (Claude), Z.ai (GLM), Kimi |
| **ChatActor** | DELETED | Removed; PromptBar routes directly to Conductor |
| **Watcher** | DISABLED | Explicitly disabled in main.rs during harness simplification |
| **React/sandbox-ui** | DELETED | Full migration to Dioxus complete |

### What Docs Claim

Several docs reference:
- React/sandbox-ui code (deleted)
- ChatActor (deleted)
- Parallel feature work (now linear roadmap)
- Watcher as authority (now de-scoped)
- Phase A/B/C milestones (superseded by roadmap-dependency-tree.md)

---

## Execution Commands

### Phase 1: DELETE Obsolete Files (Do First)

These reference deleted React code or are explicitly superseded:

```bash
# React-related (6 files, ~6,800 lines)
rm docs/COMPREHENSIVE_IMPLEMENTATION_REVIEW.md
rm docs/API_TYPE_GENERATION_REVIEW.md
rm docs/STATE_MANAGEMENT_REVIEW.md
rm docs/APP_PARITY_ANALYSIS.md
rm docs/DESKTOP_WINDOW_COMPARISON_REPORT.md
rm docs/DOCS_CLEANUP_REACT_REMOVAL_GUIDE.md

# Duplicated/superseded prompts (3 files)
rm docs/prompts/04-writer-prompt-button-live-conductor-edits.md
rm docs/prompts/05-conductor-living-document-implementation.md
rm docs/prompts/03.5.1-conductor-watcher-baml-cutover.md

# Obsolete notes (1 file)
rm docs/notes/2026-02-01-actorcode-notes.md
```

**Total: 10 files to DELETE**

---

### Phase 2: CREATE Archive Structure

```bash
mkdir -p docs/handoffs/archive/2026-Q1
mkdir -p docs/archive/prompts
mkdir -p docs/archive/notes
mkdir -p docs/archive/reports
mkdir -p docs/archive/testing
mkdir -p docs/archive/retrospectives
```

---

### Phase 3: ARCHIVE Completed Handoffs (44 files)

Move completed handoffs to archive:

```bash
# Completed infrastructure work
mv docs/handoffs/2026-02-14-writer-control-flow-checklist.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-14-regression-recovery-plan-writer-trace.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-13-writer-first-cutover-implementation-report.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-13-aggressive-writer-cutover.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-12-document-driven-multiagent.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-11-conductor-living-document-implementation.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-11-high-priority-determinism-blocked-observability.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-10-documentation-simplification.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-06-231156-phase-b-terminal-agentic-bash-transparency.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-06-terminal-multibrowser-drag-followup.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-06-dioxus-ws-stabilization-and-next-steps.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-06-dioxus-terminal-multibrowser-fix.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-06-terminal-fd-leak-fix.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-06-documentation-cleanup-and-progress-update.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-06-phase2-3-reconnect-streaming-handoff.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-06-chat-thread-management.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-phase1-foundation-complete.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-200617-r3-content-viewer-mvp-premerge.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-200626-r2-window-management-premerge-handoff.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-200555-r1-desktop-decomposition-premerge.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-151635-theme-user-global-toggle-next-steps.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-144456-chat-tool-streaming-ui-next-steps.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-123043-axum-refactor.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-terminal-ws-smoketest.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-05-terminalactor-complete.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-04-complete-actix-to-ractor-migration.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-04-ractor-event-bus-complete.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-04-eventstore-migration-in-progress.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-04-eventbus-testing-learnings.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-04-213859-frontend-apps-broken.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-event-bus-implementation.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-opencode-kimi-fix.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-142500-permissive-permissions.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-124700-research-verification.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-072140-actorcode-research-system.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-052247-actorcode-orchestration.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-020951-choir-chat-testing-phase1.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-170751-actorcode-ax-observability.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-180203-docs-upgrade-notes-bus.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-183056-docs-coherence-critique.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-02-01-docs-upgrade-runbook.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-01-31-220519-baml-chat-agent-implementation.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-01-31-desktop-complete.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-01-31-tests-complete.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-01-31-deployment-ready.md docs/handoffs/archive/2026-Q1/
mv docs/handoffs/2026-01-30-actor-architecture.md docs/handoffs/archive/2026-Q1/
```

**KEEP in docs/handoffs/ (7 active files):**
- README.md
- 2026-02-14-harness-simplification-single-source.md
- 2026-02-14-packet-e-tracing-foundation-assessment.md
- 2026-02-13-simplified-multiagent-comms-architecture.md
- 2026-02-13-llm-tracing-runbook.md
- 2026-02-13-simplified-multiagent-comms-implementation-runbook.md
- 2026-02-11-unified-agentic-harness-refactor-checklist.md

---

### Phase 4: ARCHIVE Prompts, Notes, Reports

```bash
# Prompts (6 files)
mv docs/prompts/01-pathway-readiness-and-progress-doc-normalization.md docs/archive/prompts/
mv docs/prompts/02-backend-conductor-mvp-report-to-writer.md docs/archive/prompts/
mv docs/prompts/03-prompt-bar-and-writer-auto-open.md docs/archive/prompts/
mv docs/prompts/03.5-conductor-agentic-readiness-before-writer-prompt-loop.md docs/archive/prompts/
mv docs/prompts/03.5.2-conductor-concurrent-run-narrative-checkpoint.md docs/archive/prompts/
mv docs/prompts/03.6-conductor-wake-dispatch-loop-hardening.md docs/archive/prompts/

# Notes (7 files)
mv docs/notes/2026-02-01-coherence-analysis.md docs/archive/notes/
mv docs/notes/2026-02-01-actorcode-skill-review.md docs/archive/notes/
mv docs/notes/2026-02-01-runbooks-review.md docs/archive/notes/
mv docs/notes/2026-02-01-architecture-doc-review.md docs/archive/notes/
mv docs/notes/2026-02-01-dashboard-ux-review.md docs/archive/notes/
mv docs/notes/2026-02-01-workflow-doc-review.md docs/archive/notes/
mv docs/notes/2026-02-01-pico-watcher-report.md docs/archive/notes/

# Reports (2 files)
mv docs/reports/baml_instrumentation_strategy.md docs/archive/reports/
mv docs/reports/model-agnostic-test-report.md docs/archive/reports/

# Testing (1 file)
mv docs/testing/e2e-harness-conductor-writer-plan.md docs/archive/testing/

# Retrospectives (1 file)
mv docs/retrospectives/actorcode-retrospective.md docs/archive/retrospectives/
```

**KEEP in place:**
- `docs/prompts/refactor-session-2026-02-10.md` (active)
- `docs/reports/conductor-intelligence-2026-02-10.md` (active)
- `docs/dev-blog/` (both files are current)

---

### Phase 5: ARCHIVE Old Architecture Docs

Some architecture docs are historical and should be marked:

```bash
# Historical reference (keep but these are superseded by 2026-02-14-* docs)
# Consider adding "Status: Historical" headers to:
# - 2026-02-08-architecture-reconciliation-review.md
# - 2026-02-10-conductor-run-narrative-token-lanes-checkpoint.md
# - 2026-02-10-conductor-watcher-baml-cutover.md
# - roadmap-critical-analysis.md
```

---

### Phase 6: Clean Up Empty Directories

```bash
rmdir docs/notes 2>/dev/null || true
rmdir docs/retrospectives 2>/dev/null || true
rmdir docs/testing 2>/dev/null || true
```

---

## Final Directory Structure

```
docs/
├── AGENTS.md (root level, symlink or reference)
├── DOCUMENTATION_CONSOLIDATION_RUNBOOK.md (this file, delete after execution)
├── ARCHITECTURE_SPECIFICATION.md
├── AUTOMATIC_COMPUTER_ARCHITECTURE.md
├── CHOIR_MULTI_AGENT_VISION.md
├── DESKTOP_ARCHITECTURE_DESIGN.md
├── terminal-ui.md
├── DOCUMENTATION_UPGRADE_PLAN.md
├── BUGFIXES_AND_FEATURES.md
├── websocket-implementation-review.md
├── theme-system-research.md
├── content-viewer-research.md
├── window-management-research.md
│
├── architecture/
│   ├── NARRATIVE_INDEX.md        # PRIMARY ENTRY POINT
│   ├── 2026-02-14-*.md           # Current authoritative docs
│   ├── roadmap-dependency-tree.md
│   ├── directives-execution-checklist.md
│   ├── writer-api-contract.md
│   ├── files-api-contract.md
│   ├── backend-authoritative-ui-state-pattern.md
│   ├── logging-watcher-architecture-design.md
│   ├── adr-0001-eventstore-eventbus-reconciliation.md
│   └── ... (other keepers)
│
├── handoffs/
│   ├── README.md
│   ├── 2026-02-14-harness-simplification-single-source.md
│   ├── 2026-02-14-packet-e-tracing-foundation-assessment.md
│   ├── 2026-02-13-simplified-multiagent-comms-architecture.md
│   ├── 2026-02-13-llm-tracing-runbook.md
│   ├── 2026-02-13-simplified-multiagent-comms-implementation-runbook.md
│   ├── 2026-02-11-unified-agentic-harness-refactor-checklist.md
│   └── archive/
│       └── 2026-Q1/ (44 archived files)
│
├── prompts/
│   └── refactor-session-2026-02-10.md
│
├── reports/
│   └── conductor-intelligence-2026-02-10.md
│
├── dev-blog/
│   ├── from-slop-to-signal-verified.md
│   └── 2026-02-01-why-agents-need-actors.md
│
└── archive/
    ├── prompts/
    ├── notes/
    ├── reports/
    ├── testing/
    └── retrospectives/
```

---

## Summary Statistics

| Action | Count | Lines Removed/Archived |
|--------|-------|------------------------|
| DELETE | 10 files | ~8,000 lines |
| ARCHIVE (handoffs) | 44 files | ~25,000 lines |
| ARCHIVE (other) | 17 files | ~5,000 lines |
| KEEP | ~30 files | N/A |

---

## Post-Cleanup Actions

1. **Update NARRATIVE_INDEX.md** - Remove references to deleted/archived docs
2. **Update AGENTS.md** - Ensure snapshot matches current state
3. **Create archive README** - Add `docs/archive/README.md` explaining what's archived
4. **Delete this runbook** - After execution, remove this file

---

## Files Requiring Minor Updates

| File | Update Needed |
|------|---------------|
| `BUGFIXES_AND_FEATURES.md` | Remove React migration references |
| `websocket-implementation-review.md` | Verify Dioxus paths still correct |
| `docs/dev-blog/2026-02-01-why-agents-need-actors.md` | Update file references |

---

## Lifecycle Policy (Establish Going Forward)

1. **Handoffs**: Move to `archive/YYYY-QN/` within 1 week of completion
2. **Architecture**: Add `Status: Historical` header when superseded
3. **Prompts**: Archive after implementation; keep only active prompts
4. **Notes**: Convert valuable notes to architecture docs or delete within 2 weeks
5. **Naming**: Use `YYYY-MM-DD-slug.md` format consistently
