# Actorcode Skill System Review

**Date:** 2026-02-01  
**Reviewer:** nano documentation writer  
**Purpose:** Document the actorcode skill system and assess implementation status

---

## Executive Summary

The actorcode skill suite is **fully implemented** and operational, exceeding the architecture document's Phase 1-2 scope. The system provides HTTP-first orchestration of OpenCode sessions with comprehensive observability, research automation, and dashboard capabilities.

---

## 1. Current Skill Inventory

### Implemented Skills (4 total)

| Skill | Location | Status | Language | Primary Use |
|-------|----------|--------|----------|-------------|
| **actorcode** | `skills/actorcode/` | Production-ready | Node.js/TypeScript | OpenCode session orchestration |
| **multi-terminal** | `skills/multi-terminal/` | Production-ready | Python | Tmux session management |
| **session-handoff** | `skills/session-handoff/` | Production-ready | Python | Context preservation for agent handoffs |
| **dev-browser** | `skills/dev-browser/` | Partial | Unknown | Browser automation (profiles only) |

### Skill: actorcode (Primary)

**Implementation Status:** COMPLETE (Phases 1-3)

**Core Scripts:**
- `actorcode.js` - Main CLI (714 lines) with 13 commands
- `research-launch.js` - Research task orchestration with KPIs
- `research-monitor.js` - Session monitoring and learning extraction
- `research-status.js` - Status reporting
- `findings.js` - Findings database queries
- `findings-server.js` - Web dashboard API server
- `cleanup-sessions.js` - Session cleanup utility
- `diagnose.js` - Diagnostic tooling
- `fix-findings.js` - Automated finding resolution

**Library Modules:**
- `lib/client.js` - OpenCode SDK client wrapper
- `lib/registry.js` - Session registry management (JSON-based)
- `lib/logs.js` - File-based logging system
- `lib/args.js` - CLI argument parsing
- `lib/contract.js` - Prompt contract builder
- `lib/summary.js` - Message summarization
- `lib/env.js` - Environment loading
- `lib/findings.js` - Findings persistence
- `lib/research.js` - Research task definitions

**Key Features Implemented:**
- Per-session model selection (pico/nano/micro/milli tiers)
- SSE event streaming with fallback polling
- File-based observability (registry + logs)
- Research automation with KPIs and self-verification
- Findings database with categorization
- Web dashboard (HTML + server)
- Tmux dashboard integration
- Message polling with wait capabilities

---

## 2. Skill Implementation Patterns

### Pattern 1: SKILL.md + scripts/ Structure

All skills follow a consistent layout:

```
skills/<skill-name>/
├── SKILL.md              # Primary documentation
├── docs/
│   └── usage.md          # Detailed usage examples
├── scripts/
│   ├── main-script.js    # Entry point
│   ├── lib/              # Shared modules
│   └── subcommand.js     # Sub-command scripts
└── package.json          # Dependencies (Node.js skills)
```

### Pattern 2: Justfile Integration

Skills expose commands via Justfile recipes:

```just
# actorcode
actorcode *ARGS:
    node skills/actorcode/scripts/actorcode.js {{ARGS}}

research *TEMPLATES:
    node skills/actorcode/scripts/research-launch.js {{TEMPLATES}}

# multi-terminal (Python)
# Used programmatically via import
```

### Pattern 3: Environment Configuration

**actorcode:**
- `OPENCODE_SERVER_URL` (default: http://localhost:4096)
- `OPENCODE_SERVER_USERNAME` (default: opencode)
- `OPENCODE_SERVER_PASSWORD` (optional)

**multi-terminal:**
- Uses system tmux installation
- No additional env vars required

**session-handoff:**
- Auto-detects git state
- No configuration required

### Pattern 4: File-Based State Management

**actorcode registry:**
- Location: `.actorcode/registry.json`
- Format: JSON with sessions dictionary
- Auto-recovery from corruption

**actorcode logs:**
- Location: `logs/actorcode/ses_<id>.log`
- Location: `logs/actorcode/supervisor.log`
- Format: Timestamped lines

**session-handoff:**
- Location: `docs/handoffs/YYYY-MM-DD-HHMMSS-<slug>.md`
- Format: Markdown with structured sections

---

## 3. Gap Analysis: Architecture vs Reality

### Architecture Document (actorcode_architecture.md)

The architecture doc describes a **planned** system with:
- TypeScript/Node scripts
- HTTP-first orchestration
- SSE events + polling fallback
- File-based registry
- CLI commands: spawn, status, models, message, abort, events

### Actual Implementation

The implementation **exceeds** the architecture:

| Feature | Architecture | Reality | Gap |
|---------|--------------|---------|-----|
| **Core CLI** | Planned | 13 commands | +6 commands |
| **Research System** | Not mentioned | Full KPI-driven research | NEW |
| **Findings DB** | Not mentioned | JSONL + index + queries | NEW |
| **Dashboard** | Not mentioned | Web + tmux dashboards | NEW |
| **Model Tiers** | 4 tiers | 4 tiers + descriptions | MATCH |
| **Event Streaming** | SSE preferred | SSE + polling + supervisor loop | ENHANCED |
| **Logs** | Per-session | Per-session + supervisor + follow mode | ENHANCED |
| **Message Wait** | Not mentioned | `--wait` with timeout | NEW |
| **Auto-fix** | Not mentioned | `fix-findings.js` | NEW |

### Missing from Architecture

1. **Research Automation** - The research system with KPIs is a major addition
2. **Findings Database** - Structured finding persistence with categories
3. **Dashboard Ecosystem** - Both web and tmux dashboards
4. **Cleanup Utilities** - Session and finding cleanup scripts
5. **Diagnostics** - Built-in diagnostic tooling
6. **Message Summarization** - AI-powered session summarization

### Architecture Doc Status

The architecture document is **outdated** relative to implementation. It describes Phase 1-2, but the codebase is at Phase 3+ with significant feature additions.

---

## 4. Skill System Architecture

### Overview

The skill system enables AI agents to:
1. **Orchestrate** multiple OpenCode sessions (actorcode)
2. **Manage** concurrent terminal processes (multi-terminal)
3. **Preserve** context across sessions (session-handoff)

### Component Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                        Supervisor Agent                      │
│                    (OpenCode session - you)                  │
└────────────────────┬────────────────────────────────────────┘
                     │
        ┌────────────┼────────────┐
        │            │            │
        ▼            ▼            ▼
┌──────────────┐ ┌────────┐ ┌────────────┐
│  actorcode   │ │ multi- │ │  session-  │
│   scripts    │ │terminal│ │  handoff   │
└──────┬───────┘ └───┬────┘ └─────┬──────┘
       │             │            │
       ▼             ▼            ▼
┌──────────────┐ ┌────────┐ ┌────────────┐
│ OpenCode SDK │ │  tmux  │ │   git/fs   │
│  HTTP API    │ │        │ │            │
└──────┬───────┘ └────────┘ └────────────┘
       │
       ▼
┌──────────────┐
│  Subagents   │
│ (pico/nano/  │
│ micro/milli) │
└──────────────┘
```

### Data Flow

1. **Spawn:** Supervisor → actorcode → OpenCode API → Subagent session
2. **Monitor:** Subagent → SSE events → actorcode → logs/registry
3. **Research:** actorcode → spawn research agents → findings DB
4. **Dashboard:** findings-server → dashboard.html → real-time updates
5. **Handoff:** session-handoff → git + fs → docs/handoffs/

---

## 5. Key Implementation Details

### actorcode: Model Tiers

| Tier | Model | Cost | Use Case |
|------|-------|------|----------|
| pico | zai-coding-plan/glm-4.7-flash | Lowest | Quick research, scripts |
| nano | zai-coding-plan/glm-4.7 | Low | Straightforward coding |
| micro | kimi-for-coding/k2p5 | Medium | General purpose (default) |
| milli | openai/gpt-5.2-codex | High | Complex debugging |

### actorcode: Permission Model

Default permissions for spawned agents:
```javascript
{
  edit: "allow",
  bash: "allow",
  webfetch: "allow",
  doom_loop: "ask"
}
```

### multi-terminal: Core API

```python
session = TerminalSession("name", "/path")
session.add_window("server", "npm run dev")
session.add_window("test", "npm test", split=True)
session.wait_for_pattern("server", "ready")
output = session.capture_output("test")
```

### session-handoff: Workflow

1. `create_handoff.py [slug]` - Generate scaffold
2. Fill in [TODO: ...] sections
3. `validate_handoff.py <file>` - Check quality/security
4. `check_staleness.py <file>` - Assess freshness on resume

---

## 6. Recommendations

### Immediate (High Priority)

1. **Update Architecture Document**
   - Refresh `docs/actorcode_architecture.md` to reflect current implementation
   - Document research system, findings DB, and dashboards
   - Add architecture decision records (ADRs) for major additions

2. **Create Skill Index**
   - Add `skills/README.md` with skill inventory
   - Include quickstart for each skill
   - Cross-reference AGENTS.md

3. **Document dev-browser**
   - Currently only has browser profiles
   - Either complete implementation or remove

### Short-term (Medium Priority)

4. **Standardize Error Handling**
   - actorcode has good error handling with logSupervisor()
   - Ensure all skills follow similar patterns

5. **Add Skill Tests**
   - No test files found for skills
   - Add unit tests for lib/ modules
   - Add integration tests for main commands

6. **Enhance Documentation**
   - Add troubleshooting sections to each SKILL.md
   - Document common failure modes
   - Add example workflows

### Long-term (Low Priority)

7. **Skill Registry**
   - Consider formal skill metadata (version, dependencies, etc.)
   - Auto-discovery of available skills

8. **Cross-skill Integration**
   - Document how skills compose (e.g., actorcode + multi-terminal)
   - Add helper scripts for common combinations

---

## 7. Files Referenced

### Core Documentation
- `docs/actorcode_architecture.md` - Original architecture (outdated)
- `AGENTS.md` - Development guide with skill references

### actorcode Skill
- `skills/actorcode/SKILL.md` - Main documentation
- `skills/actorcode/docs/usage.md` - Detailed usage
- `skills/actorcode/scripts/actorcode.js` - Main CLI
- `skills/actorcode/scripts/lib/` - Shared modules

### multi-terminal Skill
- `skills/multi-terminal/SKILL.md` - Main documentation
- `skills/multi-terminal/scripts/terminal_session.py` - Core implementation

### session-handoff Skill
- `skills/session-handoff/SKILL.md` - Main documentation
- `skills/session-handoff/scripts/create_handoff.py` - Scaffold generator

---

## 8. Conclusion

The actorcode skill system is **mature and production-ready**. The implementation significantly exceeds the original architecture document, adding research automation, findings management, and comprehensive observability.

**Key Strengths:**
- Well-structured skill layout
- Consistent CLI patterns via Justfile
- Robust file-based state management
- Excellent observability (logs, registry, dashboards)
- Strong integration between skills

**Areas for Improvement:**
- Architecture documentation is outdated
- Missing skill-level tests
- dev-browser skill incomplete

**Recommendation:** Update architecture docs to reflect reality, then proceed with Phase 4 enhancements (if planned).
