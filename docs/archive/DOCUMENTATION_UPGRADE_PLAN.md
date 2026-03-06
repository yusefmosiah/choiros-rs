# Documentation Upgrade Plan (Outline)

## 1) Purpose
- Align docs with current system reality and roadmap.
- Make docs verifiable, coherent, and maintainable.
- Support the producer/supervisor workflow and the actorcode dashboard.

## 2) Scope
- Core architecture docs
- Development workflow docs
- Actorcode skill docs (dashboard + CLI)
- Handoffs + progress + notes
- Research outputs and findings

## 3) Principles
- Notes are raw, learnings are derived.
- No contradictions: coherence is the minimum bar.
- Claims must be verifiable (repo-truth and/or world-truth).
- Thin top-level summaries with drill-down detail.
- Prefer append-only logs with projections for views.

## 4) Doc Taxonomy
- **Notes**: raw observations and deltas (append-only).
- **Learnings**: synthesized from notes and evidence.
- **Specs**: authoritative system design (human-gated).
- **Runbooks**: operational procedures (high trust, fast updates).
- **Design Docs**: proposals with decisions and alternatives.
- **Progress**: rolling summary of state and next steps.
- **Handoffs**: session context preservation for multi-session workflows (see `docs/handoffs/`).

## 5) Verification Lattice (Doc Accuracy)
- **Coherence**: internal consistency.
- **Repo-Truth**: matches code/config.
- **World-Truth**: matches external sources.
- **Human Gate**: for architectural claims and invariants.

## 6) Notes -> Learnings Workflow
- Capture notes via message bus events.
- Pico watchers summarize notes and signal supervisors.
- Supervisors decide when to promote notes to learnings.
- Learnings become evidence-backed entries (not raw notes).

## 7) Ownership & Update Cadence
- Assign doc owners per category.
- Define update windows (daily/weekly/release).
- Doc staleness checks and reminders.

## 8) Doc Index & Navigation
- Central index with doc categories.
- Tagging for intent (spec/runbook/notes).
- Links from progress -> specs -> evidence.

## 9) Dashboard Integration
- Whole-log and summary views for runs.
- Notes stream + findings stream (separate).
- Evidence links back to doc sections.

## 10) Deliverables (Phase 1)
- Updated core architecture doc(s) with verified claims.
- Notes stream + learnings pipeline guidance.
- Actorcode dashboard UX notes + usage guide.
- Doc index + ownership matrix.

## 11) Open Questions
- Which docs are authoritative vs historical?
- What is the minimal external validation set?
- How to gate doc updates in CI?
