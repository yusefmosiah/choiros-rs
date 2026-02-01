# Documentation Upgrade Coherence Analysis

**Date:** 2026-02-01  
**Supervisor:** docs-upgrade-supervisor  
**Scope:** Cross-cutting contradictions and overlaps across all doc slices

---

## Summary

5 nano workers completed slice reviews. This document identifies contradictions, overlaps, and coherence issues across all findings.

---

## Critical Contradictions

### 1. Port Numbers (HIGH)

| Document | Claims Port | Actual Port | Issue |
|----------|-------------|-------------|-------|
| `AUTOMATED_WORKFLOW.md` | UI :5173 | UI :3000 | Outdated Dioxus default |
| `ARCHITECTURE_SPECIFICATION.md` | Hypervisor :8001 | Not implemented | Speculative architecture |
| `ARCHITECTURE_SPECIFICATION.md` | Sandboxes :9001+ | Single :8080 | Multi-tenant not implemented |

**Resolution:** Standardize on :8080 (API), :3000 (UI) across all docs.

### 2. Technology Stack (HIGH)

| Technology | Architecture Spec | Actual Implementation | Status |
|------------|-------------------|----------------------|--------|
| Sprites.dev | Primary sandbox runtime | Never implemented | Remove all references |
| libsql | Not mentioned | Actual database driver | Add to spec |
| sqlx | Listed as SQLite driver | In workspace but unused | Clarify or remove |
| BAML version | 0.218.0 | 0.218 (workspace), 0.217 (generator) | Harmonize versions |

### 3. Actor System Architecture (MEDIUM)

| Spec Claims | Actual Implementation | Gap |
|-------------|----------------------|-----|
| `WriterActor` | `DesktopActor` manages windows | Rename/redirect |
| `BamlActor` | Direct BAML integration | Remove actor claim |
| `ToolExecutor` actor | Tools in `tools/` module | Clarify module vs actor |
| `ChatActor` only | `ChatActor` + `ChatAgent` | Document both |

### 4. Deployment Architecture (HIGH)

**Contradiction:** Architecture spec describes multi-tenant Docker deployment with hypervisor routing. Reality is single-tenant EC2 with systemd.

| Spec | Reality |
|------|---------|
| Docker Compose | No docker-compose.yml |
| Sprites adapter | Not implemented |
| Multi-sandbox (ports 9001+) | Single sandbox on :8080 |
| WebAuthn/Passkey | No authentication layer |

---

## Overlaps

### 1. Workflow Documentation (3 docs overlap)

| Document | Overlap With | Content |
|----------|--------------|---------|
| `AGENTS.md` | `AUTOMATED_WORKFLOW.md` | Dev commands, testing |
| `AUTOMATED_WORKFLOW.md` | `AGENTS.md` | Tmux workflow, dev-workflow.sh |
| `Justfile` | Both above | Commands referenced but not explained |

**Recommendation:** Consolidate into single `WORKFLOW.md` with AGENTS.md as quick reference.

### 2. Deployment Documentation (3 docs overlap)

| Document | Overlap With |
|----------|--------------|
| `DEPLOYMENT_STRATEGIES.md` | `DEPLOYMENT_RUNBOOK.md`, `DEPLOYMENT_REVIEW.md` |
| `DEPLOYMENT_RUNBOOK.md` | Strategies, Review |
| `DEPLOYMENT_REVIEW.md` | Runbook procedures |

**Recommendation:** Merge into single `docs/runbooks/deployment.md`.

### 3. Actorcode Documentation (2 docs, 1 missing)

| Document | Status | Content |
|----------|--------|---------|
| `actorcode_architecture.md` | Outdated (Phase 1-2) | Design doc |
| `SKILL.md` (in skills/actorcode/) | Current | Usage docs |
| Implementation | Phase 3+ | 13 CLI commands, dashboards |

**Gap:** No unified actorcode documentation linking design to usage.

---

## Coherence Issues

### 1. Skills System (MAJOR GAP)

**Finding:** Skills are a major workflow component (13 Justfile commands) but:
- Not mentioned in `ARCHITECTURE_SPECIFICATION.md`
- Only 2 of 4 skills listed in `AGENTS.md`
- No skill index or cross-reference

**Impact:** Agents cannot discover available skills.

### 2. Handoff System (UNDOCUMENTED)

**Finding:** 14 handoff files exist with structured workflow, but:
- Not mentioned in architecture spec
- No link between handoffs and session-handoff skill
- Validation exists but not documented

### 3. Research System (UNDOCUMENTED)

**Finding:** Actorcode research system has:
- KPI-driven task launching
- Findings database
- Web + tmux dashboards
- 11 Justfile commands

**But:** No documentation outside of `SKILL.md` and code comments.

### 4. Testing Documentation (FRAGMENTED)

**Finding:** Testing info spread across:
- `AGENTS.md` (quick commands)
- `TESTING_STRATEGY.md` (comprehensive)
- `AUTOMATED_WORKFLOW.md` (E2E)
- `PHASE5_MARKDOWN_TESTS.md` (legacy?)

**No single source of truth for test procedures.**

---

## Verification Lattice Status

Per `DOCUMENTATION_UPGRADE_PLAN.md` Section 5:

| Criterion | Status | Notes |
|-----------|--------|-------|
| **Coherence** | ⚠️ PARTIAL | Contradictions in ports, tech stack, deployment |
| **Repo-Truth** | ❌ POOR | Many claims don't match code |
| **World-Truth** | ⚠️ PARTIAL | EC2 deployment accurate, Docker not |
| **Human Gate** | ❌ MISSING | No ADRs for major decisions |

---

## Recommendations

### Immediate (Block Release)

1. **Fix port contradictions** - Standardize :8080/:3000
2. **Remove Sprites.dev references** - Never implemented
3. **Clarify hypervisor status** - Document as stub/future
4. **Add actorcode to AGENTS.md** - 13 undocumented commands

### Short-term (This Sprint)

5. **Consolidate deployment docs** - Merge 3 overlapping docs
6. **Create skills index** - Document all 4 skills
7. **Update architecture spec** - Reflect actual implementation
8. **Document research system** - Dashboards, findings DB

### Long-term (Next Quarter)

9. **Create runbook taxonomy** - Proposed structure in runbooks-review.md
10. **Write missing runbooks** - Incident response, onboarding, DB ops
11. **Establish doc maintenance** - Owners, review cycles
12. **Add ADRs** - Document key architecture decisions

---

## Files Requiring Updates

### Critical (High Priority)

- [ ] `docs/ARCHITECTURE_SPECIFICATION.md` - Remove Sprites.dev, fix ports, update actors
- [ ] `docs/AUTOMATED_WORKFLOW.md` - Fix port :5173→:3000, deprecate OpenProse
- [ ] `AGENTS.md` - Add actorcode commands, all 4 skills

### Major (Medium Priority)

- [ ] `docs/actorcode_architecture.md` - Update to Phase 3+ reality
- [ ] Merge `DEPLOYMENT_*.md` files into single runbook
- [ ] Create `skills/README.md` - Skill inventory

### Minor (Low Priority)

- [ ] Cross-link all docs
- [ ] Add "Last Updated" dates
- [ ] Archive outdated handoffs

---

## New Files Needed

1. `docs/runbooks/README.md` - Runbook index
2. `docs/runbooks/incident-response.md` - Critical gap
3. `docs/runbooks/onboarding.md` - Critical gap
4. `docs/runbooks/database-ops.md` - Critical gap
5. `skills/README.md` - Skill inventory
6. `docs/ARCHITECTURE_OVERVIEW.md` - Current state (vs v1.0 spec)

---

## Conclusion

The documentation upgrade reveals significant coherence issues. The architecture spec predates implementation and contains aspirational content never built. The workflow docs have overlapping responsibilities. The skills system (a major component) is largely undocumented in the main guides.

**Key Metric:** ~40% of architecture spec claims are inaccurate or unimplemented.

**Recommendation:** Treat ARCHITECTURE_SPECIFICATION.md as historical design doc. Create new ARCHITECTURE_OVERVIEW.md reflecting actual implementation. Consolidate overlapping workflow docs. Document the skills system comprehensively.

---

*Coherence analysis complete. Ready for supervisor decision on consolidation approach.*
