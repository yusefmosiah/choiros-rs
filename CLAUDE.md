# ChoirOS Development Guide

This repo uses **cagent** for work management, knowledge, and orchestration.

## Quick Start

```bash
# See the work graph
cagent work list
cagent work show <work-id>

# See what's ready to work on
cagent work ready

# Start autonomous dispatch
cagent supervisor --default-adapter codex

# Add a quick idea
cagent inbox "thing I noticed"

# Add operational details (private, never in git)
cagent work private-note <work-id> --text "..." --type credential
```

## Documentation

Read the docs system directly — `docs/ATLAS.md` is the auto-generated index.

- `docs/theory/` — Proposed ADRs, design notes
- `docs/practice/` — Accepted ADRs, implementation guides, runbooks
- `docs/state/` — Test reports, snapshots, audits
- `docs/archive/` — Historical, superseded

## Build & Test

```bash
just build          # cargo build
just test           # cargo test
just dev            # start local dev (hypervisor + sandbox)
just fmt            # cargo fmt + clippy
```

## Deploy

Push to main triggers CI auto-deploy to Node B (staging).
Promotion to Node A via `promote.yml` workflow.
Do NOT manually SSH-deploy — CI handles it.

## Prior Context

Architecture, operational details, and development priorities are in the
cagent work graph (`.cagent/cagent.db`). Sensitive details (SSH, credentials)
are in private notes (`.cagent/cagent-private.db`, gitignored).

The work graph was bootstrapped from the 280+ docs in `docs/` via
`cagent supervisor` with doc ontology ingestion.
