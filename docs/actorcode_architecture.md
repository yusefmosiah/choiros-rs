# Actorcode Architecture (OpenCode Orchestration)

## Goal

Create an OpenCode skill suite that orchestrates multiple OpenCode sessions through the HTTP server + SDK. The system should enable:

- Per-subagent model selection
- Supervision of progress and failures
- Bidirectional messaging between supervisor and subagents
- Low-context supervisor (state-light)
- Optional TUI automation for visual inspection (pilotty) as a fallback, not primary control

## Key Findings (Consolidated)

1. OpenCode exposes a full HTTP server and SDK.
   - Server: `opencode serve` with OpenAPI at `/doc`
   - SDK: `@opencode-ai/sdk`
   - Events: SSE stream at `/event`
2. Skills are prompt + docs. Plugins are runtime hooks.
   - Actorcode should be a skill suite with scripts (TypeScript/Node) that call the SDK.
3. TUI automation is optional.
   - `pilotty` is useful for visual inspection and TUI debugging.
   - HTTP API is the primary control plane.

## Architecture Overview

Supervisor skill (OpenCode session)
  -> actorcode scripts (Node/TS)
     -> OpenCode HTTP server (local or remote)
        -> sessions (subagents)
           -> messages + events (SSE)

## Skill Suite Layout

skills/actorcode/
  SKILL.md
  scripts/
    actorcode.ts        # CLI entrypoint (routes subcommands)
    spawn.ts            # create session + prompt_async
    supervise.ts        # SSE events + status polling fallback
    message.ts          # send and route messages
    models.ts           # per-agent model selection
    session.ts          # list/status/abort
  docs/
    usage.md            # examples + safety notes

## HTTP API Capabilities (Critical Endpoints)

Sessions:
- POST /session
- GET /session/status
- POST /session/:id/abort
- GET /session/:id/children

Messages:
- POST /session/:id/message
- POST /session/:id/prompt_async
- POST /session/:id/command
- POST /session/:id/shell

Events:
- GET /event (SSE stream)

TUI (optional):
- POST /tui/append-prompt
- POST /tui/submit-prompt
- POST /tui/execute-command

## Core Behaviors

### 1) Spawn agent with model selection

- Supervisor calls actorcode spawn
- actorcode creates a session, attaches model/agent
- Sends prompt via prompt_async

### 2) Supervision and progress

- Prefer SSE events from /event
- Fall back to /session/status polling if SSE unavailable
- Track activity by session_id and last event timestamp

### 3) Messaging

- Supervisor and agents exchange messages using /session/:id/message
- Messages include metadata: priority, intent (help, note, redirect, question)

### 4) Interrupt and redirect

- Supervisor can abort with /session/:id/abort
- Supervisor can spawn a new agent for research, then redirect original

## Supervisor Context Strategy

Keep the supervisor context extremely small:
- registry of agents: session_id, role, status, last_event_ts
- latest KPIs only, not full logs
- detailed work and history remain in each subagent session

## CLI Plan (actorcode)

Suggested commands:

- actorcode spawn --title "auth-research" --agent explore --tier micro --prompt "..."
- actorcode status
- actorcode models
- actorcode message --to <session_id> --text "..." --priority high
- actorcode abort --id <session_id>
- actorcode events --since <ts>

## Phased Implementation

Phase 1 (HTTP-first, 1-2 weeks)
- actorcode spawn/status/message/abort
- SSE event listener
- minimal registry (json file in project)

Phase 2 (Supervisor logic, 1-2 weeks)
- automatic stuck detection (no events for N seconds)
- auto-redirect to helper agent
- model fallback strategy

Phase 3 (Optional)
- pilotty for TUI inspection
- tmux integration for manual debug

## Risks and Mitigations

- Server not running -> actorcode can auto-start local server or fail with guidance
- SSE disconnects -> fall back to /session/status polling
- Rate limits -> switch model and re-prompt
- Context explosion -> supervisor stores only KPIs

## Next Step

Implement actorcode scripts using `@opencode-ai/sdk` and add SKILL.md docs for OpenCode to use.
