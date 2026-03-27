# Docs System Philosophy and Practice

Date: 2026-03-10
Kind: Guide
Status: Active
Priority: 3
Requires: [ADR-0015]
Owner: Platform / DX / Documentation

## Narrative Summary (1-minute read)

ChoirOS docs are not meant to be a passive wiki. They are a working knowledge
system for both humans and agents.

The core problem the system tries to solve is canonicality: a reader should be
able to tell what is true now, what is still being worked out, what is just
execution evidence, and what is only historical context.

The current operating model is:

1. `docs/ATLAS.md` is the entrypoint.
2. `docs/` is the best default home for implemented or currently true
   system knowledge.
3. `docs/` is for proposals, plans, explorations, and unfinished design.
4. `docs/` is for time-bound reports and snapshots.
5. `docs/archive/` is history, not default truth.

The philosophy is low-ceremony and git-native: write normal Markdown, keep it
readable raw, add minimal frontmatter, and move docs between lifecycle buckets
as reality changes.

## What Changed

1. Added one synthesized guide for exporting the docs-system model to another
   agent.
2. Combined the philosophy from ADR-0015 and the docs-v2 framing with the
   actual repo practice enforced by `ATLAS.md`, `generate-atlas.sh`, and
   `AGENTS.md`.
3. Made the current tensions explicit so readers do not confuse aspirational
   design with fully finished migration.

## What To Do Next

1. Use this guide plus `docs/ATLAS.md` as the starting packet for any new
   agent.
2. When changing docs, update the canonical file in the right lifecycle bucket
   instead of adding another parallel note.
3. Keep pruning or promoting stale `state/` and `theory/` material so the atlas
   stays useful.

## 1) What The Docs System Is For

The docs system exists to answer five questions cheaply:

1. What is ChoirOS trying to become?
2. What is true about the platform today?
3. What is changing right now?
4. Where should a new fact or design change be recorded?
5. How does a human or agent find the active story without reading the whole
   archive?

This is why the repo emphasizes canonicality over clever document mechanics.
The goal is not a fancy wiki engine. The goal is to keep truth legible while the
system changes quickly.

## 2) Core Philosophy

### 2.1 Canonicality beats category sprawl

The main failure mode is not "missing categories." It is truth being split
across too many parallel documents. A good doc placement should make lifecycle
obvious:

- true now
- proposed but not settled
- execution evidence
- historical residue

### 2.2 Markdown-first and git-native

Docs are ordinary repository artifacts:

- readable as raw Markdown
- reviewable in git
- movable through commits as their lifecycle changes
- cheap to edit during active implementation

The system explicitly avoids a heavyweight CMS or rigid schema-first workflow.

### 2.3 The filesystem is a lifecycle signal

The directory should tell you what stage a document is in. The content and
frontmatter tell you what the document means.

The current repo-wide lifecycle buckets are:

- `docs/`: thinking, proposals, draft decisions, prescriptive build
  guides, open notes
- `docs/`: implemented or in-use knowledge, accepted decisions,
  operational guides, current contracts
- `docs/`: reports, checkpoints, handoffs, load tests, snapshots
- `docs/archive/`: superseded or historical material

This is the current implementation of the broader kanban idea described in
ADR-0015. Earlier notes described the same idea as `active/` and `canon/`; the
repo has since operationalized that as `theory/` and `practice/`.

### 2.4 The atlas is a view, not the source of truth

`docs/ATLAS.md` is the entrypoint, not the canonical content itself.

Its job is to give humans and agents:

- a short current-state summary
- priority-ordered theory docs
- the current practice corpus
- recent state docs
- dependency chains reconstructed from `Requires:`

It is generated from doc frontmatter by
`scripts/generate-atlas.sh`, committed to git, and refreshed by the pre-commit
hook when docs change.

### 2.5 Docs follow the same promotion logic as code

The intended lifecycle is promotion, not duplication.

Examples:

- a draft decision starts in `docs/` and moves to
  `docs/` when accepted
- a build guide starts in `docs/` and becomes an operational
  guide in `docs/` when the system exists
- a transient test result lives in `docs/state-report-`
- a stale checkpoint eventually belongs in `docs/archive/`

Moving or revising the canonical doc is preferred over creating another
"updated-final-v2" sibling file.

## 3) Current Practice In This Repo

### 3.1 Read order for a new human or agent

Default onboarding path:

1. Read `docs/ATLAS.md`.
2. Read the specific `docs/` decisions or guides for the subsystem you
   care about.
3. Read `docs/` only for future direction, open design, or unfinished
   migration context.
4. Read `docs/` for the latest evidence or checkpoint on a live thread.
5. Read `docs/archive/` only when history matters.

### 3.2 Frontmatter is intentionally small

The working contract is minimal frontmatter near the top of each doc:

- `Date`
- `Kind`
- `Status`
- `Priority`
- `Requires`
- sometimes `Owner`

This is enough for indexing, sorting, and dependency reconstruction without
forcing authors into a heavy ontology.

### 3.3 Major docs need a human-first summary block

Per `AGENTS.md`, major architecture and roadmap docs should start with:

- `Narrative Summary (1-minute read)`
- `What Changed`
- `What To Do Next`

This rule exists because unrendered Markdown still needs to be useful in a raw
editor, terminal, or agent context.

### 3.4 Archive is not default authority

`docs/archive/` contains valuable history, but it should not be treated as the
default source of truth when an active counterpart exists.

A common failure mode is reading a highly detailed archived handoff and
mistaking it for the current contract. The safer default is:

- `practice` for truth
- `theory` for intent
- `state` for evidence
- `archive` for archaeology

## 4) Filing And Mutation Rules

When adding or updating docs, use this rule of thumb:

### Put it in `docs/` when:

- it describes an implemented or currently governing architecture
- it is the operational guide people should actually follow
- it is the contract another agent should assume by default

### Put it in `docs/` when:

- it describes a proposal, draft ADR, future design, or planned migration
- it is a prescriptive implementation guide for work still underway
- it is a note that has not earned promotion yet

### Put it in `docs/` when:

- it captures a checkpoint, handoff, benchmark, test result, or time-bound
  verification artifact
- the document is mainly "what happened in this run/session/window"

### Put it in `docs/archive/` when:

- the material is superseded
- the event is over
- the migration is done
- the document is only useful as historical context

### When in doubt

Prefer updating an existing canonical doc over adding a parallel one.

If you truly do not know where something belongs yet, start in `docs/`
or `docs/` depending on whether it is design intent or execution evidence.

## 5) Current Tensions And Known Drift

This system is real, but not perfectly settled.

Important current caveats:

1. There was not one prior single canonical "docs system philosophy and
   practice" guide; the model was spread across ADR-0015, the docs-v2 framing,
   `ATLAS.md`, and repo instructions.
2. ADR-0015 is still marked `Draft`, even though much of its topology is already
   live in the repo.
3. Some older docs and archive notes still reference the previous
   `docs/architecture/` / `NARRATIVE_INDEX` world.
4. Some files in `practice/` still carry `Active` or `Draft` statuses because
   "in use" and "fully settled" are not identical concepts.

So the safest interpretation is:

- the lifecycle model is active and should be used now
- the migration is incomplete
- the atlas and current directory structure are more authoritative than older
  archive-era filing schemes

## 6) Export Summary For Another Agent

If you need to explain the docs system in one paragraph:

ChoirOS uses a git-native docs system organized by lifecycle, not by topic
sprawl. `docs/ATLAS.md` is the entrypoint. `docs/` contains the best
current truth about implemented systems, `docs/` contains proposals and
unfinished design, `docs/` contains time-bound execution evidence, and
`docs/archive/` contains history. Docs are plain Markdown with small frontmatter
(`Date`, `Kind`, `Status`, `Priority`, `Requires`), and major docs begin with a
human-first summary block. The rule is to update or promote canonical docs as
reality changes rather than creating parallel duplicates.

## 7) Source Documents

This guide synthesizes these primary sources:

1. `docs/adr-0015-docs-kanban-architecture.md`
2. `docs/archive/2026-03-06-docs-v2-problem-framing.md`
3. `docs/ATLAS.md`
4. `scripts/generate-atlas.sh`
5. `AGENTS.md`
