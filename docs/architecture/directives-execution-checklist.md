# ChoirOS Directives App Execution Checklist

Date: 2026-02-08
Status: Active directives checklist
Owner: PromptBar + Supervisor architecture track

## North Star Deliverable

First-class operator cockpit:
- Persistent hierarchical directives view (app/window, live-updating).
- Every directive maps to actor events and policy decisions.
- Reproducible desktop runs with config-hash and model-change audit trail.

This is the primary deliverable, not a sidebar polish task.

## Hard Delineation (Must Stay True)

- ChatAgent:
  - Can call `bash` tool interface.
  - Cannot execute shell directly.
  - `bash` always delegates through `TerminalActor`.
- TerminalActor:
  - Executes commands and owns terminal agentic loop.
  - Emits detailed lifecycle/progress events.
- PromptBarActor:
  - Orchestrates actors and writes human-legible memos.
  - Cannot call tools directly.
- Supervisor:
  - Owns actor lifecycle supervision (spawn/restart/health).
  - Is not the mandatory hot path for every actor-to-actor call.
  - Prompt instructions do not grant authority.

## Policy Enforcement Pattern (Phased)

### Phase 1 (Now): Code-First Deterministic Policy
- Fast local policy checks in code on sender/receiver boundaries.
- Direct actor-to-actor calls remain the normal execution path.
- Every allow/deny emits structured policy events.
- No LLM dependency in enforcement path.

### Phase 2 (Next): Deterministic Policy Actors for Escalation
- Introduce policy actors only for risk-tiered actions (`high` risk).
- Normal `low/medium` actions continue using local checks.
- Policy actor handles escalation workflow and audit trace.

### Phase 3 (Later): Agentic Policy Advisory (Hardening)
- Agentic policy is advisory first, never sole authority initially.
- Deterministic checks remain final gate.

## Single Source of Truth

- Use one text policy/config file:
  - `config/choir_policy.toml`
- Human + agent edited by prompting.
- No button/dropdown requirement for policy changes.
- Every config/model mutation emits an event.

## Checklist (Priority Ordered)

### 0) Directives App First-Class (Top Priority)
- [ ] Add `DirectiveForest` data model in backend for hierarchical directive state.
- [ ] Add websocket stream for directive deltas (`directive.created|updated|blocked|completed`).
- [ ] Render Directives as an app/window (dockable, hide/show, mobile/desktop friendly).
- [ ] Support directive links to source events (`actor_call`, `worker_*`, `policy_*`, `model_*`).
- [ ] Add filter modes: `active`, `blocked`, `waiting`, `completed`.

Acceptance:
- Directives app is always available and can be opened quickly from prompt bar/launcher.
- Directive state survives reconnect/reload from EventStore replay.

### 1) Authority Boundary Enforcement
- [ ] Add capability policy parser for `config/choir_policy.toml`.
- [ ] Enforce `who can call whom` via shared deterministic policy checks in actor call boundaries.
- [ ] Enforce `who can call tools` in actor tool handlers.
- [ ] Deny direct shell execution outside `TerminalActor`.
- [ ] Add explicit `permission_denied` event schema.
- [ ] Add risk-tier routing (`low|medium|high`) where only `high` routes to policy actor flow.

Acceptance:
- Tests prove Chat cannot run direct shell.
- Tests prove PromptBar cannot call tools.

### 2) Bash as Terminal Transport Contract
- [ ] Keep `bash` as Chat-facing interface.
- [ ] Ensure handler path is delegation-only to TerminalActor.
- [ ] Ensure no remaining fallback path can run shell directly.
- [ ] Emit `terminal_agent_dispatch` + `terminal_tool_call/result` consistently.

Acceptance:
- All shell commands produce worker/terminal actor events and no local process path in Chat.

### 3) Text-Only Config UX
- [ ] Create initial `config/choir_policy.toml` with rich comments.
- [ ] Build settings/models app view as text renderer/editor for this file.
- [ ] Show policy version/hash and last editor in UI.
- [ ] Log `policy.changed` event with old/new hash and actor identity.

Acceptance:
- Model/policy can be changed end-to-end by editing one file through prompting flow.

### 4) Model Agnosticism Test + Report (Gate)
- [ ] Run model matrix for Bedrock, Z.ai GLM, Kimi against harness.
- [ ] Validate resolution order: request > app > user > env > fallback.
- [ ] Validate expected-fail configs fail cleanly.
- [ ] Publish report in `docs/reports/model-agnostic-test-report.md`.

Acceptance:
- Report includes pass/fail table, env requirements, and runtime caveats.

### 5) ResearcherActor v1
- [ ] Implement researcher capability actor with scoped tools.
- [ ] Add chat abstraction: `web_search` delegates to ResearcherActor.
- [ ] Emit structured research lifecycle events.
- [ ] Restrict write scope to text outputs.

Acceptance:
- Research task is observable end-to-end with citations and directive updates.

### 6) Policy Actor + Model Policy Worker
- [ ] Implement `PolicyActor` as config authority.
- [ ] Implement `ModelPolicyWorker` for runtime model resolution decisions.
- [ ] Add mutation workflow (`propose -> validate -> apply -> audit`).
- [ ] Expose read APIs for prompt/system prompt derivation.

Acceptance:
- Runtime model selection source is visible and reproducible for every LLM call.

## Event Schema Additions (Required)

- `policy.changed`
- `policy.denied`
- `model.selection`
- `model.changed`
- `directive.created`
- `directive.updated`
- `directive.blocked`
- `directive.completed`

Each must include:
- `correlation_id`
- `actor_id`
- `session_id`/`thread_id` when applicable
- `policy_hash` (once config system lands)

## Stop Conditions (Do Not Drift)

Pause feature work if any are true:
- Directives app is not available as a persistent control surface.
- Authority enforcement exists only in prompts, not code.
- Chat can still run shell outside TerminalActor path.
- Model/config changes are not auditable by event trail.
