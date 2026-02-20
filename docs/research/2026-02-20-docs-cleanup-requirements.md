# Docs Cleanup Requirements (libsql -> sqlx)

Date: 2026-02-20
Owner: Platform/Docs
Status: Draft for execution

## Narrative Summary (1-minute read)

ChoirOS runtime code has moved from `libsql` to `sqlx` for SQLite access, but documentation still
contains mixed terminology and stale migration guidance. This document defines a repeatable cleanup
policy: update active docs to current state (`SQLite via sqlx`), preserve historical context where
needed, and explicitly mark legacy references instead of leaving ambiguous wording.

## What Changed

- Established scope tiers for documentation updates (active, historical-active, archive).
- Defined canonical wording for current database architecture.
- Defined rules for handling intentional historical references to `libsql`.
- Defined acceptance checks for finishing this cleanup.

## What To Do Next

- Update Tier 1 and Tier 2 docs immediately in a single cleanup pass.
- Keep Tier 3 archives mostly intact unless they are actively referenced for onboarding.
- Run validation scans and record any intentional residual references.

## Problem Statement

Current docs include stale statements such as:

- "SQLite/libsql backend" as current architecture.
- Nix guidance that assumes `libsql` C-build/linker behavior.
- Migration tasks written as pending even though migration is already completed.

This creates incorrect operator guidance, onboarding friction, and confusing roadmap status.

## Objectives

1. Ensure active docs describe the current backend correctly: `SQLite via sqlx`.
2. Remove stale "pending migration" language from active planning docs.
3. Preserve historical records without presenting legacy state as current truth.
4. Keep unrendered markdown readable and concise.

## Scope Tiers

### Tier 1 (Must update now)

- Operational and onboarding docs:
  - `/Users/wiz/choiros-rs/README.md`
  - `/Users/wiz/choiros-rs/docs/runbooks/nix-setup.md`
  - `/Users/wiz/choiros-rs/docs/TESTING_STRATEGY.md`
  - `/Users/wiz/choiros-rs/docs/ARCHITECTURE_SPECIFICATION.md`
  - `/Users/wiz/choiros-rs/docs/design/watcher-actor-architecture.md`
  - `/Users/wiz/choiros-rs/docs/security/choiros-logging-security-report.md`

### Tier 2 (Update for consistency)

- Active architecture/research/progress artifacts that are still consulted:
  - `/Users/wiz/choiros-rs/docs/architecture/2026-02-17-codesign-runbook.md`
  - `/Users/wiz/choiros-rs/docs/architecture/NARRATIVE_INDEX.md`
  - `/Users/wiz/choiros-rs/docs/architecture/roadmap-dependency-tree.md`
  - `/Users/wiz/choiros-rs/docs/architecture/roadmap-critical-analysis.md`
  - `/Users/wiz/choiros-rs/docs/architecture/RLM_INTEGRATION_REPORT.md`
  - `/Users/wiz/choiros-rs/progress.md`
  - `/Users/wiz/choiros-rs/roadmap_progress.md`
  - `/Users/wiz/choiros-rs/docs/research/event-storage-strategy-2026-02-08.md`
  - `/Users/wiz/choiros-rs/docs/research/MULTIWRITER_EDITOR_INTEGRATION.md`

### Tier 3 (Historical/archive)

- Archive and handoff docs may keep legacy wording if clearly historical.
- If edited, add a short qualifier such as "historical context from pre-2026-02-18 migration state."

## Canonical Terminology Rules

1. Current runtime wording:
   - Preferred: `SQLite via sqlx`
   - Acceptable: `sqlx-backed SQLite`
2. Migration status wording:
   - Preferred: `libsql -> sqlx migration completed on 2026-02-18`
3. Avoid in active docs:
   - `libsql backend` (without historical qualifier)
   - `libsql C deps` as current Nix blocker
4. Code/API snippets:
   - Replace `libsql` API examples with `sqlx` equivalents in active docs.
   - If retaining legacy snippets, mark them explicitly as legacy.

## Acceptance Criteria

1. Tier 1 docs contain no unqualified `libsql` references.
2. Tier 2 docs either:
   - use current `sqlx` wording, or
   - explicitly mark `libsql` mentions as historical migration context.
3. No active runbook claims `libsql -> sqlx` is still pending.
4. Validation scan output is reviewed and intentional residuals are documented.

## Validation Commands

```bash
cd /Users/wiz/choiros-rs
rg -n "\blibsql\b|SQLite/libsql|libsql/SQLite" README.md progress.md roadmap_progress.md docs
```

Optional strict check (after Tier 1/Tier 2):

```bash
cd /Users/wiz/choiros-rs
rg -n "\blibsql\b" README.md docs/ARCHITECTURE_SPECIFICATION.md docs/TESTING_STRATEGY.md docs/runbooks/nix-setup.md
```

## Risk Notes

- Over-aggressive replacement can erase important historical rationale in handoffs and archives.
- Security and research reports may contain legacy example snippets; convert or annotate deliberately.
- Nix guidance must be revalidated after text changes so dependency notes remain technically accurate.
