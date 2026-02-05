# Actorcode Prototype Retrospective + Removal Plan

**Date:** February 4, 2026  
**Status:** Draft - Pending Review

---

## Context

We built an OpenCode-based multiagent prototype (`actorcode`) to orchestrate subagents, research runs, and dashboards. It does not provide actor messaging primitives (mailboxes, event bus, subscriptions), which are central to ChoirOS. There is no feasible fix inside OpenCode for this.

We will keep using OpenCode's built-in blocking subagent tasks for now, then migrate to our own actor system when messaging primitives land.

---

## Why Actorcode Failed (Root Causes)

1. **No actor messaging primitives** - No real event bus, mailbox semantics, or subscriptions
2. **Polling-based state** - Log tailing required for basic state, doesn't scale, brittle
3. **Non-durable sessions** - OpenCode sessions aren't actor lifecycles; supervision was ad-hoc
4. **File-based observability** - Depended on logs/registries instead of canonical event stream
5. **Parallel UI** - Dashboard on port 8765 created separate system, not integrated with ChoirOS
6. **Permission blocking** - Prompts created blocking states requiring custom handling

---

## Key Learnings Worth Keeping

- **Non-blocking supervisors essential** - Workers can block, supervisors must not
- **Incremental reporting works** - `[LEARNING] CATEGORY: ...` protocol created usable streaming feedback
- **File-based persistence useful** - JSONL logs good for audits and dashboards
- **Detached monitor pattern** - Good for aggregation without blocking main flow
- **Permissive permissions correct** - When isolation exists; prompts stall work
- **Dashboards necessary** - But should be built on core event bus, not separate logs

---

## What to Retire

- `actorcode` CLI and custom registry/log system
- `actorcode` dashboard (HTML + findings server)
- Justfile recipes implying actorcode is the path forward
- Doc references treating actorcode as bridge/core component

---

## What to Carry Forward into ChoirOS

- Supervisor/worker separation and "never block supervisor" rule
- Incremental findings protocol and categories (BUG, SECURITY, DOCS, REFACTOR, PERFORMANCE, ARCHITECTURE)
- File-based artifact persistence, but from event bus, not ad-hoc logs
- "Monitor" concept as first-class WatcherActor

---

## Removal Plan

### Phase 1: Skill Implementation (Immediate)
- [ ] Remove `skills/actorcode/` directory entirely
- [ ] Update `skills/system-monitor/SKILL.md` (remove actorcode refs)
- [ ] Update `skills/multi-terminal/SKILL.md` (remove actorcode refs)

### Phase 2: Build Integration (Immediate)
- [ ] Remove actorcode recipes from `Justfile`
- [ ] Remove actorcode scripts references

### Phase 3: Documentation (This Week)
- [ ] Update `AGENTS.md` - Replace actorcode concurrency guidance with OpenCode built-in tasks
- [ ] Update `docs/CHOIR_MULTI_AGENT_VISION.md` - Remove actorcode as target, clarify OpenCode interim
- [ ] Update `docs/AUTOMATIC_COMPUTER_ARCHITECTURE.md` - Remove actorcode references
- [ ] Update `docs/DOCUMENTATION_UPGRADE_PLAN.md` - Remove actorcode skill docs task
- [ ] Archive `docs/archive/actorcode_architecture.md` (already archived)

### Phase 4: Historical Records (Decision Needed)
- [ ] Decide: Delete vs archive historical notes
- [ ] Move to `docs/archive/actorcode-retrospective/` or delete

### Phase 5: Artifacts (After Confirmation)
- [ ] Delete `.actorcode/` directory
- [ ] Delete `logs/actorcode/` directory
- [ ] Clean up `docs/research/nixos-research-2026-02-01/supervisor.log.jsonl`

---

## Removal Inventory

### Skill Implementation
- `skills/actorcode/` (entire directory)
- `skills/actorcode/SKILL.md`
- `skills/actorcode/docs/usage.md`
- `skills/actorcode/scripts/`
- `skills/actorcode/dashboard.html`
- `skills/actorcode/dashboard/`
- `skills/actorcode/package.json`

### Referencing Skills
- `skills/system-monitor/SKILL.md`
- `skills/multi-terminal/SKILL.md`

### Build/Run Shortcuts
- `Justfile` (actorcode recipes)

### Core Docs
- `AGENTS.md` (task concurrency section)
- `docs/CHOIR_MULTI_AGENT_VISION.md`
- `docs/AUTOMATIC_COMPUTER_ARCHITECTURE.md`
- `docs/DOCUMENTATION_UPGRADE_PLAN.md`
- `docs/archive/actorcode_architecture.md`

### Historical Records (Decision Needed)
- `docs/notes/2026-02-01-actorcode-skill-review.md`
- `docs/notes/2026-02-01-actorcode-notes.md`
- `docs/notes/2026-02-01-pico-watcher-report.md`
- `docs/handoffs/2026-02-01-052247-actorcode-orchestration.md`
- `docs/handoffs/2026-02-01-072140-actorcode-research-system.md`
- `docs/handoffs/2026-02-01-124700-research-verification.md`
- `docs/handoffs/2026-02-01-142500-permissive-permissions.md`
- `docs/handoffs/2026-02-01-170751-actorcode-ax-observability.md`
- `docs/handoffs/2026-02-01-docs-upgrade-runbook.md`
- `docs/dev-blog/2026-02-01-why-agents-need-actors.md`
- `progress.md`

### Artifacts
- `.actorcode/` (registry + findings)
- `logs/actorcode/`
- `docs/research/nixos-research-2026-02-01/supervisor.log.jsonl`

---

## Open Questions

1. **Historical records**: Delete or archive? (Recommend: Archive key learnings, delete implementation details)
2. **Learnings protocol**: Keep `[LEARNING]` convention for OpenCode tasks? (Recommend: Yes, until native event bus)
3. **Retro doc format**: One combined doc or separate retro + removal plan? (This doc combines both)

---

## Next Steps

1. Review this retrospective
2. Decide on historical records approach
3. Execute removal plan phases
4. Update AGENTS.md with new concurrency guidance
5. Focus development on TerminalActor and native ChoirOS dashboard

---

*The actorcode prototype served its purpose: it taught us what we actually need. Now we build it properly in ChoirOS.*
