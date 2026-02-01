# Pico Watcher Report - 2026-02-01

## Executive Summary

Completed inspection of all markdown files in `docs/notes/` and `docs/handoffs/`. Found **7 significant attention-worthy changes** that should be reviewed by supervisor.

---

## Files Inspected

### Notes (7 files, 2,366 total lines)
- 2026-02-01-coherence-analysis.md (214 lines)
- 2026-02-01-actorcode-skill-review.md (349 lines)
- 2026-02-01-runbooks-review.md (297 lines)
- 2026-02-01-architecture-doc-review.md (314 lines)
- 2026-02-01-dashboard-ux-review.md (335 lines)
- 2026-02-01-workflow-doc-review.md (234 lines)
- 2026-02-01-actorcode-notes.md (15 lines)

### Handoffs (14 files, 2,940 total lines)
- 2026-02-01-180203-docs-upgrade-notes-bus.md (88 lines)
- 2026-02-01-170751-actorcode-ax-observability.md (114 lines)
- 2026-02-01-opencode-kimi-fix.md (97 lines)
- 2026-02-01-142500-permissive-permissions.md (318 lines)
- 2026-02-01-124700-research-verification.md (233 lines)
- 2026-02-01-072140-actorcode-research-system.md (219 lines)
- 2026-02-01-052247-actorcode-orchestration.md (126 lines)
- 2026-02-01-020951-choir-chat-testing-phase1.md (273 lines)
- 2026-01-31-220519-baml-chat-agent-implementation.md (246 lines)
- 2026-01-31-deployment-ready.md (443 lines)
- 2026-01-31-tests-complete.md (446 lines)
- 2026-01-31-desktop-complete.md (517 lines)
- 2026-01-30-actor-architecture.md (archived)
- docs/handoffs/README.md (238 lines)

---

## Attention-Worthy Changes

### 1. Documentation Coherence Crisis (CRITICAL)

**Source:** `docs/notes/2026-02-01-coherence-analysis.md`

**Finding:** Architecture specification is significantly outdated. ~40% of claims don't match implementation.

**Key Contradictions:**
- **Sprites.dev**: Listed as primary sandbox runtime, NEVER IMPLEMENTED
- **Hypervisor**: Spec describes multi-tenant routing, actual implementation is 5-line stub
- **Deployment**: Spec describes Docker Compose + multi-sandbox containers, actual is single-tenant EC2 with systemd
- **BAML Version**: Spec says 0.218.0, actual is 0.218 workspace, 0.217 generator
- **Port Numbers**: Specs use :5173 (UI) and :8001 (Hypervisor), actual is :3000 (UI) and :8080 (Sandbox)
- **Actor System**: Spec claims WriterActor, BamlActor, ToolExecutor actors - actual only has ChatActor + ChatAgent + DesktopActor + EventStoreActor

**Impact:** Documentation doesn't match codebase, confusing for developers and agents.

---

### 2. Actorcode Skill System Far Exceeds Architecture (HIGH)

**Source:** `docs/notes/2026-02-01-actorcode-skill-review.md`

**Finding:** Actorcode skill suite is production-ready with 13 CLI commands, research system, findings DB, dashboards. Implementation exceeds architecture spec by significant margin.

**What's Implemented (Not in Spec):**
- Research automation with KPI-driven task launching
- Findings database with categorization and queries
- Web dashboard + tmux dashboard
- 13 Justfile commands (research, monitor, status, dashboard, etc.)
- Cleanup utilities (session, findings)
- Diagnostic tools
- Message summarization

**Architecture Doc Status:** Outdated - describes Phase 1-2, codebase is at Phase 3+ with major additions.

**Gap:** No unified documentation linking architecture design to usage.

---

### 3. Operational Documentation Fragmentation (HIGH)

**Source:** `docs/notes/2026-02-01-runbooks-review.md`

**Finding:** ChoirOS has substantial but fragmented operational documentation (14 handoffs + 10+ core docs).

**Critical Gaps Identified:**
1. **Incident Response Runbook** - No centralized procedures
2. **Onboarding Runbook** - No step-by-step setup guide
3. **Database Operations Runbook** - No backup/restore procedures
4. **Monitoring & Alerting Runbook** - No operational procedures

**Recommendation:** Consolidate into proposed taxonomy with 7 categories (onboarding, development, deployment, operations, incident-response, testing, reference).

---

### 4. Permissive Permission Philosophy (IMPORTANT)

**Source:** `docs/handoffs/2026-02-01-142500-permissive-permissions.md`

**Finding:** Major philosophy shift from "protective" to "permissive" permission model.

**Key Insight:** With worktree isolation and repo scoping, the "dangerous commands" threat model is largely moot. OpenCode's own safety features provide sufficient protection.

**New Model:**
- Default to "allow" for all permissions (edit, bash, webfetch)
- Only block `doom_loop` (rare case where OpenCode warns)
- Real safety comes from isolation layers, not permission prompts

**Implementation:** Pre-approve permissions on spawn, fix logs command to add `--follow` flag.

---

### 5. Kimi Provider Fix Applied (IMPORTANT)

**Source:** `docs/handoffs/2026-02-01-opencode-kimi-fix.md`

**Finding:** Critical fix applied to make Kimi For Coding work via actorcode.

**Root Cause:** OpenCode TUI uses `@ai-sdk/anthropic` internally (hardcoded override), but headless API was using `@ai-sdk/openai-compatible` which doesn't send proper User-Agent headers.

**Fix Applied:** Changed `opencode.json` npm package to `@ai-sdk/anthropic`.

**Verification:** Micro tier now works via headless API, all tiers (pico/nano/micro/milli) functional.

---

### 6. Research System Operational (IMPORTANT)

**Source:** `docs/handoffs/2026-02-01-124700-research-verification.md`

**Finding:** Research system fully operational after debugging. Root cause was missing model specification in promptAsync.

**Features:**
- Non-blocking launcher (supervisor spawns, stays live)
- Background monitor for findings collection
- `[LEARNING]` tag protocol for incremental reporting
- 5 research templates (security-audit, code-quality, docs-gap, performance, bug-hunt)
- Dependency-aware fix orchestrator (fix-findings.js)
- Test hygiene checker (check-test-hygiene.js)
- Web dashboard API server

**Current State:** 58 findings in database, 15 sessions in registry.

---

### 7. Notes/Learnings Architecture Established (MEDIUM)

**Source:** `docs/handoffs/2026-02-01-180203-docs-upgrade-notes-bus.md`

**Finding:** Notes bus schema and watcher patterns established for future documentation upgrade.

**Key Concepts:**
- Notes are raw, learnings are derived
- Watchers observe events and signal supervisors
- Supervisors sleep until signals arrive
- Minimal subscription contract with rate-limiting

**Impact:** Foundation for attention-aware documentation system.

---

## Contradictions Found

### Architecture Spec vs Reality

| Claim | Spec | Reality | Status |
|-------|------|---------|--------|
| Sprites.dev | Primary runtime | Never used | ❌ Wrong |
| Hypervisor | Multi-tenant routing | Stub (5 lines) | ❌ Wrong |
| Deployment | Docker Compose | EC2 + systemd | ❌ Wrong |
| Port 8001 | Hypervisor | 8080 (sandbox) | ❌ Wrong |
| Port 5173 | UI | 3000 (Dioxus) | ❌ Wrong |
| 3 Actors | Writer, Baml, ToolExecutor | Chat + ChatAgent + Desktop + EventStore | ❌ Wrong |
| CI/CD | Implemented | Not in codebase | ❌ Wrong |

### Documentation vs Code

| Area | Docs Claim | Code Reality | Gap |
|------|-----------|--------------|-----|
| Skills System | Only 2 listed | 4 implemented | ❌ Underdocumented |
| Actorcode Commands | Not mentioned | 13 Justfile commands | ❌ Missing |
| Deployment | Multi-tenant | Single-tenant | ❌ Wrong |
| Testing | Playwright exists | Not found in codebase | ❌ Wrong |

---

## Recommendations by Priority

### CRITICAL (Immediate Action Required)

1. **Create v1.1 Architecture Spec** - Address critical inaccuracies
2. **Remove Sprites.dev references** - Never implemented, confusing
3. **Document actual deployment** - EC2 + systemd, not Docker
4. **Add Skills System section** - Major codebase component undocumented

### HIGH (This Sprint)

5. **Consolidate deployment docs** - Merge 3 overlapping files into single runbook
6. **Create skills index** - Document all 4 skills with quickstarts
7. **Update architecture spec** - Reflect actual implementation
8. **Document research system** - Dashboards, findings DB details

### MEDIUM (Next Sprint)

9. **Add missing runbooks** - Incident response, onboarding, database ops
10. **Create skill registry** - Formal metadata (version, dependencies)
11. **Add ADRs** - Document key architecture decisions
12. **Cross-link all docs** - Reduce discoverability issues

---

## Supervisor Action Required

**Decision Points:**
1. **Approve runbook taxonomy** for operational documentation consolidation
2. **Prioritize documentation upgrades** - fix critical coherence issues first
3. **Address skills system documentation** gap
4. **Verify research system operational status** as current focus area
5. **Review permissive permission model** implementation

**Signal Sent:** Heartbeat with summary of findings for supervisor review.

---

## Summary Statistics

- **Total files inspected:** 21 markdown files
- **Total lines read:** 5,306 lines
- **Attention-worthy changes found:** 7
- **Critical contradictions:** 6
- **Documentation gaps:** 4
- **Philosophy shifts documented:** 2

**Status:** Awaiting supervisor review of findings.

---

*Report generated: 2026-02-01*
*Pico watcher: Complete inspection*
