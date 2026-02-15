# Prompt 01: Pathway Readiness + Progress Doc Normalization

You are working in `/Users/wiz/choiros-rs`.

## Goal
Normalize roadmap/progress docs so they match the current reality:
- Files v0 and Writer v0 are complete and working.
- Next target is `Prompt Bar -> Conductor -> capability actors -> markdown report -> open Writer in preview mode`.
- Chat remains compatibility surface (do not make Chat primary orchestration).

## Read First
- `/Users/wiz/choiros-rs/roadmap_progress.md`
- `/Users/wiz/choiros-rs/progress.md`
- `/Users/wiz/choiros-rs/docs/architecture/roadmap-dependency-tree.md`
- `/Users/wiz/choiros-rs/docs/architecture/directives-execution-checklist.md`
- `/Users/wiz/choiros-rs/docs/architecture/backend-authoritative-ui-state-pattern.md`
- `/Users/wiz/choiros-rs/AGENTS.md`

## Tasks
1. Produce a short “doc consistency diff” section inside `roadmap_progress.md` and `progress.md`:
   - what is stale
   - what is authoritative now
   - explicit next 3 milestones
2. Make `roadmap_progress.md` authoritative for the next implementation lane:
   - milestone A: Conductor backend MVP for report generation
   - milestone B: Prompt bar routing to conductor
   - milestone C: Writer auto-open in markdown preview
3. Keep `Model-Led Control Flow` explicit in both docs.
4. Keep existing historical notes, but clearly mark them archived/non-authoritative.
5. Do not change code in this pass.

## Acceptance
- Both docs have `Narrative Summary / What Changed / What To Do Next`.
- No contradiction between `roadmap_progress.md` and `roadmap-dependency-tree.md` for immediate next steps.
- Clear statement: “Chat is compatibility; Conductor is orchestrator.”

## Validation
- Run markdown readability check by visually scanning raw md.
- Show exact sections added/edited in final summary.
