# Capability Ownership and Tool Schema Matrix (Canonical)

Date: 2026-02-14  
Status: Canonical ownership boundary  
Scope: Runtime capability ownership and single-source tool schema governance

## Narrative Summary (1-minute read)

ChoirOS uses strict capability ownership to keep authority clear.

Conductor is orchestration-only and should not execute tools directly.
Tool schemas are defined once and reused across agents that are granted access.
Terminal and Researcher include file tools as a permanent baseline.

App agents run interactive sessions.
Workers execute bounded concurrent tasks.

## What Changed

1. Explicitly removed direct tool execution authority from Conductor.
2. Added single-source rule for tool schema definitions (no per-agent duplication).
3. Set baseline grants: Terminal + Researcher always include file tools.
4. Added ownership boundaries for current and near-term app agents/workers.

## What To Do Next

1. Complete Writer app-agent harness as canonical living-document mutation authority.
2. Remove any remaining Conductor direct tool-execution paths from active runtime authority.
3. Move tool schemas to a single shared contract used by all granted agents.
4. Add tests that fail on duplicated tool definitions, missing default worker file grants,
   and Conductor direct tool execution.

---

## Ownership Matrix

1. **Shared Tool Schema Registry (single source)**
   - Owns: canonical tool definitions (`bash`, `file_read`, `file_write`, `file_edit`, and future tools).
   - Rule: define each tool schema once; grant usage per agent/worker via capability policy.

2. **Default Worker Grant Profile**
   - Terminal: `bash` + `file_read` + `file_write` + `file_edit`.
   - Researcher: `web_search`/`fetch_url` + `file_read` + `file_write` + `file_edit`.

3. **Conductor**
   - Owns: cross-agent orchestration, priorities, budgets, cancellation, routing rails.
   - Does not own: direct tool execution (`file`, `shell`, `web` tools).

4. **Writer App Agent**
   - Owns: canonical living-document/revision mutation path and file-centric interaction UX.
   - May use shared file tools through granted capability policy.

5. **Tracer App Agent**
   - Owns: trace-query and run-inspection interaction surfaces (through tracing APIs).
   - Role: observability interaction authority.

6. **Coder App Agent**
   - Owns: interactive coding session state and domain decomposition.
   - Delegates execution to: Terminal + Researcher workers as subagents.

7. **Terminal Worker**
   - Owns: bounded shell execution.
   - Uses shared file tools by default (`file_read`, `file_write`, `file_edit`).
   - Does not own: orchestration authority or document canon authority.

8. **Researcher Worker**
   - Owns: bounded external information gathering and synthesis tasks.
   - Uses shared file tools by default (`file_read`, `file_write`, `file_edit`).
   - Does not own: orchestration authority or document canon authority.

## File Mutation Rule (Hard Boundary)

1. Conductor must not directly execute tools.
2. Tool schemas are defined once and reused by all granted agents/workers.
3. Terminal and Researcher include file tools as baseline capability.
4. Writer is canonical for living-document/revision mutation authority.
5. Workers can produce outputs/patch intents and use shared file tools for bounded tasks.

## Acceptance Signals

1. No active runtime path allows Conductor direct tool execution.
2. Tool schema definitions exist in one shared contract source.
3. Terminal and Researcher receive file tools as permanent baseline capability.
4. Writer harness is canonical for living-document/revision mutations.
5. Event traces show capability grants and actor attribution per tool call.
