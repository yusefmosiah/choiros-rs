# ADR-0015: Documentation Kanban Architecture

Date: 2026-03-06
Kind: Decision
Status: Draft
Priority: 5
Requires: []
Authors: wiz + Claude

## Narrative Summary (1-minute read)

ChoirOS documentation has a canonicality problem: stable architecture, proposed changes,
operational state, and historical artifacts all live in the same flat directories. It's
unclear which doc is the truth, which is aspirational, and which is stale.

The fix is a two-column kanban: `active/` (pending) and `canon/` (done). The directory
tells you the lifecycle stage. Documents flow from active to canon when promoted. Two
auxiliary directories — `state/` for time-bound execution artifacts and `archive/` for
history — sit off the main board.

Five document types (Decision, Guide, Report, Snapshot, Note) are distinguished by
frontmatter metadata, not directory path. The directory answers one question only:
"has this been promoted yet?"

## What Changed

- Replaced the 10-category wiki proposal with a 2-column kanban model
- Collapsed "proposals" and "plans" — a plan is just a Draft Decision + prescriptive Guide
- Identified five document types by their lifecycle characteristics, not topic
- Established that docs follow the same promotion pipeline as code (ADR-0013)

## Context

### The Problem

`docs/architecture/` contains 38 files mixing accepted decisions, draft proposals,
execution plans, status docs, and implementation guides. A reader cannot tell from
the path whether a document describes what IS or what SHOULD BE.

The NARRATIVE_INDEX partially solves navigation but not canonicality — it lists docs
in reading order but doesn't distinguish their lifecycle stage.

### Previous Attempt

The docs-v2 problem framing proposed 10 wiki categories (concepts, systems, features,
decisions, proposals, roadmaps, runbooks, status, glossary, atlas). This was rejected
as overengineered — the categories overlap and don't encode the lifecycle.

### The Insight

Documentation is a kanban board. The only transition that matters is promotion:
`active/` → `canon/`. A Decision gets accepted. A Guide's feature gets built.
The directory change IS the event.

This mirrors the change lifecycle in ADR-0013: code flows through propose → test →
promote. Docs flow through active → canon. Same pipeline, same primitives.

## Decision

### Directory Structure

```
docs/
  canon/                # promoted: the truth
    decisions/          # accepted ADRs
    guides/             # operational guides for existing systems
    reports/            # durably useful reference reports (rare)
  active/               # not yet promoted: work in progress
    decisions/          # draft/proposed ADRs
    guides/             # prescriptive build guides, checklists
    notes/              # thoughts, observations, problem framings
  state/                # time-bound execution artifacts (off-board)
    snapshots/          # checkpoints, handoffs
    reports/            # test results, benchmarks, research output
  archive/              # history (off-board)
```

### Document Types

Encoded in frontmatter `Kind:` field, not in directory path.

| Type | Purpose | Update frequency | Typical lifespan |
|------|---------|-----------------|-----------------|
| **Decision** | Why we're doing something, what we chose | Slow (revise on strategy change) | Long (until superseded) |
| **Guide** | How to build it → how to operate it | Fast (checklist per subtask) | Long (until system changes) |
| **Report** | Results of doing something | Once (at creation) | Weeks to months |
| **Snapshot** | Where things are right now | Once (at creation) | Days to weeks |
| **Note** | Everything else | Variable | Variable |

### Frontmatter Contract

Every doc gets minimal frontmatter:

```yaml
# Title
Date: YYYY-MM-DD
Kind: Decision | Guide | Report | Snapshot | Note
Status: Draft | Active | Accepted | Superseded | Archived
Priority: 1-5          # attention ordering (1 = highest, optional)
Requires: [list]        # dependency chain — doc IDs this depends on
```

The directory tells you lifecycle stage. Frontmatter tells you everything else.
Content is the substance. This separation keeps the filesystem clean and the
operational state rich.

Resist adding directories for operational states. If you want `blocked/` or
`in-review/`, encode it as a frontmatter field, not a folder. The filesystem
is the attention surface, not the state machine.

### Promotion Rules

- **Decision:** Draft → Proposed → Accepted (moves `active/decisions/` → `canon/decisions/`)
- **Guide:** prescriptive build guide → reference ops guide (moves `active/guides/` → `canon/guides/`)
- **Report:** stays in `state/reports/` unless durably useful → `canon/reports/`
- **Snapshot:** stays in `state/snapshots/` → `archive/` when stale
- **Note:** stays in `active/notes/` → promotes to Decision/Guide, or → `archive/`

### Atlas (Generated Index)

`docs/ATLAS.md` replaces NARRATIVE_INDEX as the single entry point. It is:

- **Auto-generated** by `scripts/generate-atlas.sh` from doc frontmatter
- **Committed** in git (always readable without running tools)
- **Refreshed** on every commit via pre-commit hook (`.githooks/pre-commit`)
- **Manually runnable** via `just atlas`

The atlas contains everything an agent (or human) needs to proceed:
- System summary and quick-start
- Canon docs (the truth — accepted decisions, operational guides)
- Active docs (priority-ordered, with dependency chains)
- State docs (latest snapshots and reports)
- Dependency graph (reconstructed from `Requires:` fields)
- Doc counts per column

The atlas does NOT include `archive/` — that's off-board by design.

### What This Replaces

- `docs/architecture/` (flat mix) → split across `canon/decisions/` and `active/decisions/`
- `docs/runbooks/` → `canon/guides/` or `active/guides/`
- `docs/checkpoints/` → `state/snapshots/`
- `docs/handoffs/` → `state/snapshots/`
- `docs/reports/` → `state/reports/`
- `docs/design/` → `active/notes/`
- NARRATIVE_INDEX → curated view over `canon/`, generated or maintained separately

## Consequences

### Positive
- Path tells you lifecycle stage instantly (no reading frontmatter to know if it's aspirational)
- Promotion is a visible git event (file moves in a commit)
- Natural archiving — stale docs have a clear home
- Low ceremony — just write, file in `active/`, promote when ready
- Agents can scope reads to `canon/` for truth, `active/` for context

### Negative
- Migration effort for existing ~75 active docs
- Git history for moved files requires `git log --follow`
- NARRATIVE_INDEX needs reworking or replacement

### Risks
- Over-filing: spending time on where to put things instead of writing
- Mitigation: when in doubt, put it in `active/notes/`. Sort later.

## Verification

- [ ] All accepted ADRs live in `canon/decisions/`
- [ ] All draft/proposed ADRs live in `active/decisions/`
- [ ] All operational guides live in `canon/guides/`
- [ ] No doc in `canon/` has Status: Draft
- [ ] No doc in `active/` has Status: Accepted
- [ ] Frontmatter on every doc includes Kind and Status
- [ ] NARRATIVE_INDEX updated or replaced
