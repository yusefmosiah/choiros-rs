# Handoff: Actorcode AX + Observability Focus

## Session Metadata
- Created: 2026-02-01 17:07:51
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~1.5 hours

### Recent Commits (for context)
  - (no new commits in this session)

## Handoff Chain
- **Continues from**: None
- **Supersedes**: None

## Current State Summary
- Updated `progress.md` with AX/observability focus and new next steps.
- Established new rule: background runs must output a single Markdown doc (no in-task summary).
- Confirmed producer role is already possible via supervisor-spawned runs; observability/messaging becomes critical.
- Observability issue observed: `--latest` can return only the user prompt due to race; need better whole-log + summary views.

## Codebase Understanding

### Architecture Overview
- Actorcode runs are OpenCode sessions with HTTP orchestration; observability is via logs + messages + registry.
- Dashboard needs whole-log and summary views to avoid latest-only brittleness.

### Critical Files
| File | Purpose | Relevance |
| --- | --- | --- |
| `progress.md` | Current status + next steps | Updated with AX/observability focus |
| `skills/actorcode/scripts/actorcode.js` | Actorcode CLI | Messages/logs/observability behavior |
| `skills/actorcode/dashboard.html` | Web dashboard | Target for whole-log/summary views |

### Key Patterns Discovered
1. `--latest` is race-prone; without `--wait` it can return only the prompt.
2. Background runs need a document artifact for review and verification.

## Work Completed

### Tasks Finished
- [x] Updated `progress.md` with AX + observability priorities
- [x] Set new rule: background runs output Markdown docs only

### Files Modified/Created
| File | Changes | Rationale |
| --- | --- | --- |
| `progress.md` | Added AX/observability notes + next steps | Surface dashboard needs and background run contract |
| `docs/handoffs/2026-02-01-170751-actorcode-ax-observability.md` | New handoff | Capture decisions + next actions |

### Decisions Made
| Decision | Options Considered | Rationale |
| --- | --- | --- |
| Background runs output a single Markdown doc | Inline summary vs doc artifact | Enables verification + archival and avoids partial summaries |
| Dashboard must show whole-log + summaries | Latest-only view | Improves observability and avoids prompt-only races |

## Pending Work

### Immediate Next Steps
1. **Implement dashboard whole-log + summary views** (CRITICAL)
   - Whole-log markdown view for any run
   - Pico summaries for recent context
2. **Improve actorcode observability** (HIGH)
   - Add a “summary” or “timeline” mode for messages
   - Expose these views in `skills/actorcode/dashboard.html`
3. **Confirm background run contract** (HIGH)
   - Background runs must emit a single Markdown doc (no inline summary)

### Blockers/Open Questions
- None identified; focus is on observability UX.

### Deferred Items
- AX contract spec + fusion protocol formalization
- Doc accuracy verifier automation

## Context for Resuming Agent

### Important Context
**1. Observability is now the top priority.**
- Producer role can be achieved by supervisors spawning new runs; visibility must improve.

**2. Background run outputs are now document-first.**
- Require a single Markdown doc as the output artifact.

### Assumptions Made
- Dashboard will be the primary UX for whole-log + summary views.

### Potential Gotchas
- `--latest` without `--wait` can return only the user prompt.
- Message summaries must be generated without blocking the main thread.

## Environment State

### Tools/Services Used
- Actorcode CLI (HTTP OpenCode orchestration)

### Active Processes
- `ses_3e4ce1371ffe1hRi8h2eJXqCCK` (micro) - omo-background-scan
- `ses_3e4c5342effeLa9Voqlqsa9UEj` (nano) - omo-background-scan-nano
- `ses_3e4dc0ec5ffeaM58txHG3hvALU` - choir-docs-reconcile

### Environment Variables (Names Only)
- OPENCODE_SERVER_URL
- OPENCODE_SERVER_USERNAME
- OPENCODE_SERVER_PASSWORD

## Related Resources
- `skills/actorcode/dashboard.html`
- `skills/actorcode/scripts/actorcode.js`

---

**Security Reminder**: No secrets in this handoff. All API keys are in `.env` files (not committed).
