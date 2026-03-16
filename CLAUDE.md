# ChoirOS

This repo uses **cagent** for work management, knowledge, and orchestration.

```bash
cagent work list                    # see all work items (ADRs, features, tasks)
cagent work show <work-id>          # details, notes, attestations
cagent work ready                   # what's actionable now
cagent supervisor --default-adapter codex  # autonomous dispatch
cagent inbox "idea"                 # quick capture
```

The work graph was bootstrapped from 280+ docs in `docs/`. Each ADR is a work item.
Theory ADRs are `ready` (planned). Practice ADRs are `done` (implemented).
Test reports are attestation records. Guides are notes on their ADR.

Private operational details (SSH, credentials, deploy) are in
`cagent work private-note` — stored in `.cagent/cagent-private.db` (gitignored).
