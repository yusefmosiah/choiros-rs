# Handoff: Documentation Upgrade + Notes Bus Schema

## Session Metadata
- Created: 2026-02-01 18:02:03
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~1 hour

### Recent Commits (for context)
- b2f9a68 docs: capture actorcode AX observability focus

## Handoff Chain
- **Continues from**: `docs/handoffs/2026-02-01-170751-actorcode-ax-observability.md`
- **Supersedes**: None

## Current State Summary
- User wants a fundamental documentation upgrade as the next deliverable.
- Notes should be captured and routed through the message bus, not via CLI.
- Watchers (pico) observe all events and signal supervisors only when needed.
- Supervisors sleep until a watcher signal arrives.

## Notes Bus Schema (v0.1)

### Core Events
- `note.created`
  - `note_id`, `text`, `author`, `channel`, `context`, `timestamp`
- `watcher.signal`
  - `signal_id`, `watcher_id`, `priority`, `reason`, `evidence`, `target`, `timestamp`
- `supervisor.wake`
  - `wake_id`, `trigger`, `intent`, `timestamp`

### Subscription Contract (minimal)
- `watcher.subscribe`
  - `watcher_id`, `filter`, `priority`, `rate_limit`, `timestamp`
- `watcher.unsubscribe`
  - `watcher_id`, `timestamp`

### Filter Grammar (draft)
- `event_types`: array of event type strings
- `channels`: optional note channels
- `priority_min`: optional (low/medium/high)
- `tags`: optional freeform tag list
- `match`: optional regex/substring

### Rate-Limit Policy
- `max_signals_per_minute`
- `cooldown_ms`
- `burst`

## Work Completed
- Captured user’s note on notes/learnings separation.
- Defined bus schema for notes and watcher signaling (see above).

## Pending Work

### Immediate Next Steps (Documentation Upgrade)
1. **Write the Documentation Upgrade Plan**
   - Scope: core docs, handoffs, progress, skills, design docs, dashboard guide.
   - Define doc ownership, verification, and update cadence.
2. **Draft “Doc Accuracy as Verifier” spec**
   - Coherence (self-consistency) + repo-truth + world-truth.
3. **Introduce “Notes → Learnings” flow**
   - Notes are raw, learnings are derived.
   - Add watcher rules for note-to-learning synthesis.

### Blockers/Open Questions
- None; waiting on documentation upgrade draft.

## Context for Resuming Agent

### Important Context
- User wants notes to be first-class and distinct from learnings.
- Watchers signal supervisors, supervisors sleep by default.
- Documentation overhaul is the next concrete deliverable.

### Potential Gotchas
- Avoid conflating notes with learnings in docs.
- Keep watcher signals sparse via rate limits.

## Related Resources
- `docs/notes/2026-02-01-actorcode-notes.md`
- `progress.md`
- `skills/actorcode/dashboard.html`

---

**Security Reminder**: No secrets in this handoff. All API keys are in `.env` files (not committed).
