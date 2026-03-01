# Handoff: Actorcode Research System - Incremental Learnings & Non-Blocking Architecture

## Session Metadata
- Created: 2026-02-01 07:21:40
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~2 hours

### Recent Commits (for context)
  - 1cba789 fix: resolve all clippy errors and warnings
  - 89f8be1 fix: additional clippy fixes for borrowed_box and unused_must_use
  - b58588b style: formatting fixes for baml_client and tests
  - bcb6301 fix: resolve clippy errors and formatting issues
  - 4af5850 progress.md

## Handoff Chain

- **Continues from**: [2026-02-01-052247-actorcode-orchestration.md](./2026-02-01-052247-actorcode-orchestration.md)
  - Previous title: Actorcode Orchestration (HTTP-first OpenCode Control)
- **Supersedes**: None

> Review the previous handoff for full context before filling this one.

## Current State Summary

We built a non-blocking research task system for actorcode that allows the supervisor to spawn multiple research agents and continue working while subagents report findings incrementally. The key insight is that subagents can safely block on main for coordination (they're the workers), but the supervisor must stay live to orchestrate. We implemented `[LEARNING]` tags for incremental reporting, a detached monitor process to collect findings, and updated the prompt contract to enforce this protocol. Two research tasks are currently running (security-audit and code-quality), and the system is ready for the next phase: research-status command, auto-spawn monitor, shared findings database, and dashboard creation.

## Codebase Understanding

### Architecture Overview

**Hierarchical Context Engineering Pattern:**
- **Supervisor (main thread)**: Must stay live, orchestrates, non-blocking
- **Subagents (worker threads)**: Can block on main, do sequential work, report incrementally
- **Monitor (background process)**: Detached, collects `[LEARNING]` tags from all sessions

**Research Task Flow:**
1. Supervisor calls `research-launch.js` → spawns OpenCode sessions → exits immediately
2. Subagents receive prompt with `[LEARNING] CATEGORY: description` protocol
3. Subagents report findings as they discover them (don't wait for end)
4. Optional `research-monitor.js` runs detached, parses learnings, logs to files
5. Subagents mark completion with `[COMPLETE]`

**Key Files:**
- `skills/actorcode/scripts/research-launch.js` - Non-blocking task launcher
- `skills/actorcode/scripts/research-monitor.js` - Background findings collector
- `skills/actorcode/scripts/lib/contract.js` - Prompt contract with incremental reporting
- `skills/actorcode/scripts/actorcode.js` - Main CLI with `messages --require-text` and `supervisor` loop

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| `skills/actorcode/scripts/research-launch.js` | Spawns research tasks non-blocking | Entry point for automated research |
| `skills/actorcode/scripts/research-monitor.js` | Background process collecting `[LEARNING]` tags | Essential for aggregating findings |
| `skills/actorcode/scripts/lib/contract.js` | Prompt contract builder | Enforces incremental reporting protocol |
| `skills/actorcode/scripts/lib/research.js` | ResearchTask class (blocking version) | Not currently used, kept for reference |
| `skills/actorcode/scripts/actorcode.js` | Main CLI | Has `messages`, `supervisor` commands |
| `Justfile` | Task shortcuts | Added `research` and `research-monitor` recipes |
| `skills/actorcode/docs/usage.md` | Usage documentation | Documented research system |

### Key Patterns Discovered

**Incremental Reporting Protocol:**
```
[LEARNING] CATEGORY: Brief description of finding
Categories: BUG, SECURITY, DOCS, REFACTOR, PERFORMANCE, ARCHITECTURE
[COMPLETE] - marks task done
```

**Non-Blocking Spawn Pattern:**
- Use `child_process.spawn` with `detached: true` for background monitors
- Main process exits immediately after spawning
- Subagents run in their own OpenCode sessions (separate processes)

**Prompt Contract Evolution:**
- Started with basic context (repo, date, model)
- Added incremental reporting requirements
- Added `[LEARNING]` tag format specification
- Future: Add supervisor session ID for bidirectional messaging

## Work Completed

### Tasks Finished

- [x] Fixed registry corruption handling (atomic writes + auto-backup)
- [x] Implemented `messages` command with `--wait`, `--require-text`, `--latest`
- [x] Implemented `supervisor` loop command (events + status refresh)
- [x] Updated prompt contract with incremental reporting
- [x] Created `research-launch.js` (non-blocking task launcher)
- [x] Created `research-monitor.js` (background findings collector)
- [x] Added 5 research templates: security-audit, code-quality, docs-gap, performance, bug-hunt
- [x] Updated micro model to `kimi-for-coding/k2p5`
- [x] Added Justfile shortcuts: `just research`, `just research-monitor`
- [x] Updated documentation

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| `skills/actorcode/scripts/actorcode.js` | Added `messages` command with filters, `supervisor` loop, helper functions | Enable monitoring and incremental reporting |
| `skills/actorcode/scripts/lib/contract.js` | Added incremental reporting protocol to prompt contract | Enforce subagent reporting behavior |
| `skills/actorcode/scripts/lib/registry.js` | Atomic writes, corrupt backup handling | Prevent registry corruption crashes |
| `skills/actorcode/scripts/research-launch.js` | New file: non-blocking research launcher | Core of research system |
| `skills/actorcode/scripts/research-monitor.js` | New file: background findings collector | Aggregate learnings from multiple sessions |
| `skills/actorcode/scripts/lib/research.js` | New file: ResearchTask class (blocking) | Reference implementation, not currently used |
| `Justfile` | Added `research` and `research-monitor` recipes | Easy CLI access |
| `skills/actorcode/docs/usage.md` | Documented research system, messages command, supervisor | User documentation |
| `skills/actorcode/SKILL.md` | Updated command list | Quick reference |
| `skills/multi-terminal/SKILL.md` | Added actorcode debug dashboard example | Cross-skill documentation |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Non-blocking launcher vs blocking | Blocking with async/await vs spawn detached | Supervisor must stay live; subagents can block |
| Learning tag format | JSON vs plain text `[LEARNING]` | Plain text easier to parse, human readable in logs |
| Monitor as separate process vs integrated | Integrated in supervisor vs standalone | Standalone allows supervisor to do other work |
| 5 research templates | More vs fewer categories | Covers common code review needs without overwhelming |
| Prompt contract vs runtime enforcement | Contract in prompt vs code checks | Contract is lightweight, works with existing OpenCode |

## Pending Work

### Immediate Next Steps

1. **Implement `research-status` command** - Show all active research tasks with their current state (running/completed), number of learnings found, last activity timestamp
2. **Auto-spawn monitor** - When launching research with `--monitor` flag, automatically start monitor in background and return monitor PID
3. **Shared findings database/filesystem** - Design storage strategy for research findings (SQLite, JSON files, or append-only log). Research strategies: SQLite for querying, JSON for simplicity, append-only for audit trail. Run experiments comparing write performance and query capabilities
4. **Research dashboard (tmux)** - Create tmux session showing: active research tasks grid, live learnings feed, summary statistics
5. **Web dashboard (single HTML file)** - Design self-contained HTML dashboard that reads from findings database and displays: task list, learning categories chart, recent findings, search/filter

### Blockers/Open Questions

- [ ] **Findings storage format**: SQLite vs JSON vs append-only log? Need to research trade-offs
- [ ] **Dashboard data refresh**: Polling vs WebSocket vs Server-Sent Events? WebSocket might require backend changes
- [ ] **Research task lifecycle**: How long should completed tasks stay in status? When to archive?
- [ ] **Bidirectional messaging**: Should supervisor be able to send follow-up questions to research agents mid-task?

### Deferred Items

- Research findings aggregation and trending (needs shared storage first)
- Auto-nudge for stalled research tasks (needs better stall detection)
- Research task templates from user-defined prompts (needs template system)
- Integration with GitHub issues for automatic bug filing

## Context for Resuming Agent

### Important Context

**CRITICAL: The research system is designed around non-blocking supervisor + blocking subagents.**

- `research-launch.js` exits immediately after spawning - this is intentional
- Subagents work in separate OpenCode sessions and can take minutes/hours
- Use `just actorcode status` to see all sessions
- Use `just actorcode messages --id <id> --role assistant --latest` to check progress
- Two research tasks are currently running:
  - `ses_3e6f8de11ffeQiQLicdWQX2UE1` - Security audit
  - `ses_3e6f8de08ffelJI8JRRSYx7kTD` - Code quality review

**The `[LEARNING]` protocol is the key innovation:**
- Subagents must report findings as they discover them
- Format: `[LEARNING] CATEGORY: description`
- Categories: BUG, SECURITY, DOCS, REFACTOR, PERFORMANCE, ARCHITECTURE
- Monitor parses these from message text

**Current tmux sessions:**
- `actorcode-orch` - 3 windows: logs (4 panes), events, control
- `actorcode-debug` - 5 windows from earlier debugging
- Multiple other sessions for different ChoirOS components

### Assumptions Made

- OpenCode server running on localhost:4096
- Subagents will follow the `[LEARNING]` protocol (enforced via prompt contract)
- Research tasks complete within reasonable time (no timeout implemented yet)
- Findings are text-based and fit in message parts
- Single supervisor per project (no distributed coordination needed yet)

### Potential Gotchas

- **Registry corruption**: Already handled with atomic writes + backup, but monitor should handle corrupt registry gracefully
- **Message parsing**: `[LEARNING]` regex might miss malformed tags; consider stricter validation
- **Monitor crashes**: If monitor dies, learnings are still in session logs but not aggregated
- **Session explosion**: Each research task creates a new session; need cleanup strategy for old completed tasks
- **Model availability**: `kimi-for-coding/k2p5` requires valid credentials; fallback to other models if unavailable
- **Prompt length**: Full prompt with contract might exceed limits for very long tasks; consider chunking

## Environment State

### Tools/Services Used

- OpenCode server (`opencode serve`) on port 4096
- Node.js 20+ for actorcode scripts
- tmux for terminal multiplexing
- Just (justfile runner) for task shortcuts

### Active Processes

- OpenCode server: port 4096 (background)
- Research sessions: `ses_3e6f8de11ffeQiQLicdWQX2UE1`, `ses_3e6f8de08ffelJI8JRRSYx7kTD` (OpenCode subagents)
- tmux sessions: actorcode-orch, actorcode-debug, and others

### Environment Variables

- `OPENCODE_SERVER_URL` (default: http://localhost:4096)
- `OPENCODE_SERVER_USERNAME`
- `OPENCODE_SERVER_PASSWORD`

## Related Resources

- Previous handoff: [2026-02-01-052247-actorcode-orchestration.md](./2026-02-01-052247-actorcode-orchestration.md)
- Actorcode architecture: `docs/actorcode_architecture.md`
- Usage docs: `skills/actorcode/docs/usage.md`
- Multi-terminal skill: `skills/multi-terminal/SKILL.md`

---

**Security Reminder**: Before finalizing, run `validate_handoff.py` to check for accidental secret exposure.
