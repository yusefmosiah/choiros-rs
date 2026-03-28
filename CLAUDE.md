# ChoirOS

This repo uses **cogent** for work management, knowledge, and orchestration.

```bash
cogent work list                                       # see all work items (ADRs, features, tasks)
cogent work show <work-id>                             # canonical review bundle for one work item
cogent work ready                                      # what's actionable now
cogent work claim <work-id>                            # lease one ready item explicitly
cogent work doc-set --file docs/adr-XXXX.md --path docs/adr-XXXX.md
cogent work note-add <work-id> --text "operator note"
cogent work attest <work-id> --method test --result passed --summary "verification summary"
cogent serve --auto                                    # start UI/API plus autonomous dispatch
cogent inbox "idea"                                    # quick capture
```

The work graph was bootstrapped from 280+ docs in `docs/`. Each ADR is a work item.
Theory ADRs are `ready` (planned). Practice ADRs are `done` (implemented).
Test reports are attestation records. Guides are notes on their ADR.

Private operational details (SSH, credentials, deploy) are in
`cogent work private-note` — stored in `.cogent/cogent-private.db` (gitignored).
