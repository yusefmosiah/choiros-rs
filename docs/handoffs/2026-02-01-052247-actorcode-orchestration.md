# Handoff: Actorcode Orchestration (HTTP-first OpenCode Control)

## Session Metadata
- Created: 2026-02-01 05:22:47
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~2 hours
- Focus: Build actorcode skill suite with OpenCode HTTP SDK + observability

### Recent Commits (for context)
- 2084209 feat: Chat App Core Functionality - WebSocket, Icon Click, Message Flow

## Handoff Chain

- **Continues from**: [2026-02-01-020951-choir-chat-testing-phase1.md](./2026-02-01-020951-choir-chat-testing-phase1.md)
  - Previous title: ChoirOS Chat App Testing Initiative - Phase 1 Complete
- **Supersedes**: None

## Current State Summary

Actorcode is implemented as a skill suite to orchestrate OpenCode sessions over the HTTP server + SDK (no TUI automation required). It supports per-session model selection, lightweight supervision via logs, and a minimal registry. We consolidated prior research docs into a single architecture doc: `docs/actorcode_architecture.md`.

## Architecture Overview

- **Control plane**: OpenCode HTTP server (`opencode serve`) + SDK (`@opencode-ai/sdk`)
- **CLI**: `skills/actorcode/scripts/actorcode.js`
- **Registry**: `.actorcode/registry.json` (session metadata)
- **Logs**:
  - `logs/actorcode/supervisor.log`
  - `logs/actorcode/<session_id>.log`
- **Primary operations**: spawn, status, message, abort, events (SSE), logs, attach

## Key Decisions

1. **HTTP-first orchestration** over OpenCode server API (preferred to pilotty)
2. **Skill suite** (not plugin) to keep usage aligned with OpenCode workflows
3. **Observability** via per-session log files + supervisor log
4. **Model tiers** with fixed list and labels for fast escalation

## Model Tiers (fast → pricey)

1. **pico** → `zai-coding-plan/glm-4.7-flash`
   - Text-only. Run scripts/tools and quick research; not for writing new code.
2. **nano** → `zai-coding-plan/glm-4.7`
   - Text-only. Coding-capable worker for straightforward changes.
3. **micro** → `opencode/kimi-k2.5-free`
   - Multimodal (text+image). General-purpose, resource-efficient default.
4. **milli** → `openai/gpt-5.2-codex`
   - Multimodal (text+image). Long-context + debugging heavy lifting.

Default tier: **pico**.

## Work Completed

### Added
- `skills/actorcode/` skill suite with scripts and docs
- `docs/actorcode_architecture.md` (consolidated research)
- `just actorcode` shortcut

### Updated
- `skills/actorcode/SKILL.md` and `skills/actorcode/docs/usage.md` with tier docs
- `skills/actorcode/scripts/actorcode.js` with tier support + `models` command
- `Justfile` with `actorcode` recipe

## Files Modified / Added

| File | Purpose |
|------|---------|
| `skills/actorcode/scripts/actorcode.js` | Main CLI for HTTP orchestration |
| `skills/actorcode/scripts/lib/*` | args, client, logs, registry helpers |
| `skills/actorcode/SKILL.md` | Skill documentation |
| `skills/actorcode/docs/usage.md` | Usage examples |
| `docs/actorcode_architecture.md` | Consolidated architecture doc |
| `Justfile` | `just actorcode` shortcut |

## How to Run

1. Install dependencies:
   ```bash
   cd skills/actorcode
   npm install
   ```
2. Start OpenCode server:
   ```bash
   opencode serve
   ```
3. Spawn an agent (tiered model):
   ```bash
   just actorcode spawn --title "research" --agent explore --tier micro --prompt "Summarize the auth flow."
   ```
4. List models:
   ```bash
   just actorcode models
   ```
5. Watch events or logs:
   ```bash
   just actorcode events --session <session_id>
   just actorcode logs --id <session_id>
   ```

## Immediate Next Steps

1. **Demo run**: start one subagent per tier under the same supervisor and verify:
   - model selection works
   - logs write to `logs/actorcode/`
   - events stream updates registry
2. **Observability sanity check**:
   - tail `logs/actorcode/supervisor.log`
   - verify per-session logs update on message/response
3. **Optional**: add a `just opencode-serve` helper for convenience

## Open Questions / Risks

- Need to verify SSE `/event` stability under long-running sessions
- Ensure registry and logs are cleaned/rotated as sessions accumulate

## Environment Variables (names only)

- `OPENCODE_SERVER_URL`
- `OPENCODE_SERVER_USERNAME`
- `OPENCODE_SERVER_PASSWORD`

## Notes

We deleted older research drafts and replaced them with `docs/actorcode_architecture.md` to keep the narrative focused.
