# Docs as Kanban: The Two-Column Model

Date: 2026-03-06
Kind: Note
Status: Active
Requires: []

## The Insight

The docs filesystem is a kanban board with two columns:

- **`active/`** = pending (being thought about, proposed, built)
- **`canon/`** = done (accepted, operational, the truth)

Everything else is off-board:
- **`state/`** = execution artifacts (snapshots, reports — time-bound, never on the main board)
- **`archive/`** = history (superseded, completed, abandoned — fell off the board)

## Why Two Columns

The only transition that matters is promotion: `active/` → `canon/`. That's the event.
A Decision gets accepted. A Guide's feature gets built. A Note crystallizes into
something durable. The directory change IS the promotion.

More columns (backlog, review, staging, etc.) are overhead for a small team.
You either haven't done it yet, or you have. Pending or done.

## Document Types

Five types, distinguished by frontmatter metadata, not directory:

| Type | What it is | Lifecycle |
|------|-----------|-----------|
| **Decision** | Why we're doing something, what we chose (ADR) | Long-lived. Slow to update. |
| **Guide** | How to build it (prescriptive) → how to operate it (reference) | Fast-updating. Living checklist. |
| **Report** | Results of doing something (benchmarks, test results, research) | Weeks to months. |
| **Snapshot** | Where things are right now (checkpoints, handoffs) | Days to weeks. Stale fast. |
| **Note** | Everything else (thoughts, observations, problem framings) | Variable. Inbox/scratch. |

## The Feature Working Set

For each feature, the working docs are roughly:

- **Decision** — why, what we chose (slow-updating, revise when strategy changes)
- **Guide** — how to build/operate (fast-updating, checklist ticked per subtask, revised mid-stream)
- **Note** (optional) — musings, open questions, rubber-ducking

The Guide is the living work surface. The Decision is the stable frame.
Notes are the scratch pad.

When an obstacle forces a replan, the Guide updates immediately. If the replan
changes the "why," the Decision updates too.

## Type Lifecycle by Directory

| Type | In `active/` | In `canon/` | In `state/` |
|------|-------------|------------|-------------|
| Decision | Draft/Proposed | Accepted | — |
| Guide | Prescriptive ("how to build") | Reference ("how to operate") | — |
| Report | — | Promoted if durably useful | Default home |
| Snapshot | — | — | Default home |
| Note | Default home | Rare (durable system description) | — |

## The Flow

```
New idea → active/notes/
  ↓ crystallizes
Draft ADR → active/decisions/
  ↓ accepted
Accepted ADR → canon/decisions/

Build plan → active/guides/
  ↓ feature ships
Ops runbook → canon/guides/

Thought → active/notes/
  ↓ becomes irrelevant
  → archive/

Benchmark → state/reports/
  ↓ becomes baseline reference
  → canon/reports/ (rare)

Checkpoint → state/snapshots/
  ↓ superseded
  → archive/
```

## Connection to the Codebase

This is the same pattern as the change lifecycle in ADR-0013:
- Code changes flow through: propose → test → promote → rollback if needed
- Doc changes flow through: active/ → canon/ (or archive/)

The docs follow the same promotion pipeline as the code.
Because they are the same thing: artifacts moving through a lifecycle.

## Historical Context

The filesystem IS the organizational pattern. Filing systems, from Mesopotamian
clay tablets to Unix inodes, solve the same problem: how do you preserve, locate,
authorize, update, and share information across space, time, and hierarchy?

The two-column kanban is the simplest expression of that discipline:
it hasn't been promoted yet, or it has.

## Directory Structure

```
docs/
  canon/                # done: accepted decisions, reference guides, durable reports
    decisions/          # accepted ADRs
    guides/             # operational guides for existing systems
    reports/            # durably useful reports (rare promotions)
  active/               # pending: drafts, plans, notes, explorations
    decisions/          # draft/proposed ADRs
    guides/             # prescriptive build guides, checklists
    notes/              # thoughts, observations, problem framings
  state/                # off-board: time-bound execution artifacts
    snapshots/          # checkpoints, handoffs
    reports/            # test results, benchmarks, research output
  archive/              # off-board: history
```

## Minikanban: Why Two Columns, Not More

Many workflow schemas (GTD, full Kanban, etc.) are cognitive prostheses for human
memory limits. Categories like "Waiting For" or "In Review" compensate for the fact
that humans can't reliably track all dependencies and follow-ups.

For a system where agents read and write docs, those categories become unnecessary
as top-level directories. An agent doesn't need a `blocked/` folder — it can read
a `Requires:` field and compute reachability.

**Design maxim: don't encode operational complexity into the filesystem topology
unless that topology is the product.**

The directory answers: is this pending or done?
Frontmatter answers: what's its priority, what does it depend on, who owns it?
Content answers: what is it and why does it matter?

Resist the urge to add directories when process evolves. If you feel like creating
`active/blocked/` or `active/in-review/`, encode it as a field instead.

## Dependency Chains in Frontmatter

Each doc declares what it depends on via `Requires:`:

```yaml
# ADR-0014: Per-User Storage
Kind: Decision
Status: Draft
Priority: 1
Requires: [ADR-0013]
```

The dependency graph is implicit — reconstructed by reading `Requires:` across all
docs. A chain is `A requires B requires C`. A tree is `A requires [B, C]`.

No tooling needed. Any script or agent can parse YAML-ish frontmatter and build the
graph. The convention is simple enough that humans maintain it without friction.

### Example: P0-P7 Dependency Chain

```
P0 (VM persistence)   Requires: []
P1 (writer bugs)      Requires: []
P2 (fleet-ctl)        Requires: [P0]
P3 (runtime config)   Requires: []
P4 (multitenancy)     Requires: [P0, P2]
P5 (inner dev loop)   Requires: [P4]
P6 (promotion)        Requires: [P5]
P7 (benchmarks)       Requires: [P0]
```

Priority is a separate field — it encodes attention ordering, not dependency.
Something can be high priority but blocked by a dependency. The two are orthogonal.

## Frontmatter Contract

Every doc gets:

```yaml
# Title
Date: YYYY-MM-DD
Kind: Decision | Guide | Report | Snapshot | Note
Status: Draft | Active | Accepted | Superseded | Archived
Priority: 1-5          # attention ordering (1 = highest)
Requires: [list]        # dependency chain (doc IDs)
```

The directory tells you lifecycle stage. The frontmatter tells you everything else.
Content is the substance. This separation keeps the filesystem clean and the
operational state rich.
