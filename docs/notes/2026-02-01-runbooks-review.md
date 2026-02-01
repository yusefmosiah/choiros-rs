# ChoirOS Runbooks Review

**Date:** 2026-02-01  
**Reviewer:** Nano Documentation Writer  
**Status:** Review Complete - Supervisor Review Required

---

## Executive Summary

ChoirOS has a substantial but fragmented operational documentation landscape. While core procedures exist (deployment, testing, development workflow), they are scattered across multiple files with varying quality and completeness. This review identifies 14 existing documents, 4 critical gaps, and provides a recommended taxonomy for organizing operational knowledge.

---

## 1. Current Runbook Inventory

### A. Core Runbooks (High Quality)

| Document | Location | Quality | Last Updated | Purpose |
|----------|----------|---------|--------------|---------|
| **DEPLOYMENT_RUNBOOK** | `docs/archive/DEPLOYMENT_RUNBOOK_2026-01-31.md` | ⭐⭐⭐⭐⭐ | 2026-01-31 | Complete server provisioning, deployment, rollback |
| **DEPLOYMENT_STRATEGIES** | `docs/DEPLOYMENT_STRATEGIES.md` | ⭐⭐⭐⭐⭐ | 2026-01-31 | Current + future deployment options |
| **DEPLOYMENT_REVIEW** | `docs/DEPLOYMENT_REVIEW_2026-01-31.md` | ⭐⭐⭐⭐⭐ | 2026-01-31 | Critical issues, remediation plan, hardening |
| **TESTING_STRATEGY** | `docs/TESTING_STRATEGY.md` | ⭐⭐⭐⭐⭐ | 2026-01-31 | Multi-layered testing pyramid, E2E, CI/CD |
| **AUTOMATED_WORKFLOW** | `docs/AUTOMATED_WORKFLOW.md` | ⭐⭐⭐⭐ | 2026-01-31 | OpenProse agentic development, tmux workflow |

### B. Supporting Documentation (Good Quality)

| Document | Location | Quality | Notes |
|----------|----------|---------|-------|
| **AGENTS.md** | `/AGENTS.md` | ⭐⭐⭐⭐ | Quick commands, code style, project structure |
| **Justfile** | `/Justfile` | ⭐⭐⭐⭐ | Task runner commands (140 lines) |
| **Handoffs README** | `docs/handoffs/README.md` | ⭐⭐⭐⭐ | Session handoff system documentation |
| **Architecture Spec** | `docs/ARCHITECTURE_SPECIFICATION.md` | ⭐⭐⭐ | System design (needs review) |
| **Desktop Architecture** | `docs/DESKTOP_ARCHITECTURE_DESIGN.md` | ⭐⭐⭐ | UI/UX design |

### C. Operational Scripts (Executable Runbooks)

| Script | Location | Purpose | Quality |
|--------|----------|---------|---------|
| **deploy.sh** | `scripts/deploy.sh` | Production deployment | ⭐⭐⭐⭐⭐ |
| **dev-workflow.sh** | `scripts/dev-workflow.sh` | Tmux dev environment | ⭐⭐⭐⭐ |
| **provision-server.sh** | `scripts/provision-server.sh` | Server setup | ⭐⭐⭐⭐ |
| **setup-ec2-env.sh** | `scripts/setup-ec2-env.sh` | EC2 environment | ⭐⭐⭐ |

### D. Session Handoffs (14 Active)

Located in `docs/handoffs/`:
- `2026-02-01-180203-docs-upgrade-notes-bus.md` (most recent)
- `2026-02-01-170751-actorcode-ax-observability.md`
- `2026-02-01-opencode-kimi-fix.md`
- `2026-02-01-142500-permissive-permissions.md`
- `2026-02-01-124700-research-verification.md`
- `2026-02-01-072140-actorcode-research-system.md`
- `2026-02-01-052247-actorcode-orchestration.md`
- `2026-02-01-020951-choir-chat-testing-phase1.md`
- `2026-01-31-220519-baml-chat-agent-implementation.md`
- `2026-01-31-deployment-ready.md`
- `2026-01-31-tests-complete.md`
- `2026-01-31-desktop-complete.md`
- `2026-01-30-actor-architecture.md`
- `archive/2026-01-31-desktop-foundation-api-fix.md`

---

## 2. Runbook Quality Assessment

### Strengths

1. **Comprehensive Deployment Coverage**: Three deployment-related documents cover current state, future strategies, and critical issues with detailed remediation steps
2. **Testing Strategy Well-Defined**: Clear testing pyramid (70% unit, 20% integration, 10% E2E) with specific implementation patterns
3. **Executable Procedures**: Scripts are production-ready with error handling, health checks, and colored output
4. **Session Continuity**: Handoff system enables multi-session agent workflows with validation
5. **Justfile Integration**: All common tasks documented as runnable commands

### Weaknesses

1. **Documentation Sprawl**: 14 handoff files + 10+ core docs = discoverability issues
2. **Archive Confusion**: `DEPLOYMENT_RUNBOOK` is in `docs/archive/` but appears current
3. **Missing Troubleshooting**: No centralized incident response procedures
4. **No Onboarding Runbook**: New developer/agent setup not documented
5. **Inconsistent Cross-References**: Links between docs are sparse

### Quality Scores by Category

| Category | Score | Notes |
|----------|-------|-------|
| Deployment | 9/10 | Excellent coverage, executable scripts |
| Testing | 8/10 | Comprehensive strategy, some gaps in implementation |
| Development Workflow | 7/10 | Good tmux workflow, missing IDE setup |
| Incident Response | 3/10 | Scattered in handoffs, no central runbook |
| Onboarding | 2/10 | AGENTS.md has basics, no step-by-step guide |
| Security | 6/10 | Hardening in DEPLOYMENT_REVIEW, needs operationalization |

---

## 3. Missing Operational Procedures

### Critical Gaps (Priority 1)

#### A. Incident Response Runbook
**Missing**: Centralized incident response procedures
**Impact**: When production issues occur, response is ad-hoc
**Needed**:
- P0/P1/P2 severity definitions
- Escalation procedures
- Rollback decision tree
- Communication templates
- Post-incident review process

#### B. Onboarding Runbook
**Missing**: New developer/agent setup guide
**Impact**: Each new agent rediscovers setup steps
**Needed**:
- Prerequisites checklist
- Environment setup (Rust, Dioxus, tools)
- First commit walkthrough
- Local development verification
- Access requirements (AWS, GitHub, etc.)

#### C. Database Operations Runbook
**Missing**: Database backup, restore, migration procedures
**Impact**: Risk of data loss, migration errors
**Needed**:
- Backup schedule and verification
- Restore procedures with RTO/RPO
- Migration rollback procedures
- Data integrity checks

#### D. Monitoring & Alerting Runbook
**Missing**: Operational monitoring procedures
**Impact**: Issues discovered reactively
**Needed**:
- Health check endpoints and expected responses
- Log analysis procedures
- Metric thresholds and alerts
- Dashboard usage guide

### Important Gaps (Priority 2)

#### E. Security Incident Response
**Location**: Partial in `DEPLOYMENT_REVIEW.md` (hardening section)
**Missing**: Operational security procedures
**Needed**:
- Security incident classification
- Compromised credentials response
- Audit log review procedures

#### F. Release Management
**Missing**: Versioning, changelog, release notes process
**Needed**:
- Semantic versioning guidelines
- Changelog maintenance
- Release checklist
- Feature flag procedures

#### G. Capacity Planning
**Missing**: Scaling procedures and thresholds
**Needed**:
- Resource monitoring thresholds
- Scale-up/scale-down procedures
- Load testing procedures

---

## 4. Recommended Runbook Structure

### Proposed Taxonomy

```
docs/runbooks/                    # NEW: Centralized runbook directory
├── README.md                     # Runbook index and quick links
├── 00-onboarding/                # Getting started
│   ├── 01-environment-setup.md
│   ├── 02-first-contribution.md
│   └── 03-verification-checklist.md
├── 10-development/               # Daily development
│   ├── 01-local-development.md   # (from AGENTS.md + dev-workflow.sh)
│   ├── 02-testing-locally.md
│   ├── 03-debugging-guide.md
│   └── 04-common-issues.md
├── 20-deployment/                # Deployment procedures
│   ├── 01-deployment-overview.md # (merge STRATEGIES + REVIEW)
│   ├── 02-production-deploy.md   # (from DEPLOYMENT_RUNBOOK)
│   ├── 03-rollback-procedures.md
│   └── 04-server-provisioning.md # (from provision-server.sh)
├── 30-operations/                # Running production
│   ├── 01-monitoring.md          # NEW
│   ├── 02-log-analysis.md
│   ├── 03-database-ops.md        # NEW
│   └── 04-security-ops.md        # NEW
├── 40-incident-response/         # When things go wrong
│   ├── 01-severity-levels.md     # NEW
│   ├── 02-incident-response.md   # NEW
│   ├── 03-rollback-decisions.md
│   └── 04-post-incident.md       # NEW
├── 50-testing/                   # Testing procedures
│   ├── 01-testing-strategy.md    # (from TESTING_STRATEGY.md)
│   ├── 02-unit-testing.md
│   ├── 03-integration-testing.md
│   └── 04-e2e-testing.md
└── 90-reference/                 # Supporting docs
    ├── architecture.md
    ├── api-reference.md
    └── glossary.md

docs/handoffs/                    # KEEP: Session handoffs
├── README.md                     # (existing)
├── archive/                      # (existing)
└── *.md                          # (existing handoffs)

docs/notes/                       # KEEP: Working notes
└── *.md                          # (existing notes)
```

### Migration Plan

1. **Phase 1**: Create `docs/runbooks/` structure
2. **Phase 2**: Migrate and consolidate existing runbooks
3. **Phase 3**: Write missing critical runbooks (incident response, onboarding)
4. **Phase 4**: Update all cross-references
5. **Phase 5**: Archive old locations with redirects

---

## 5. Links to Relevant Code/Scripts

### Deployment
- **deploy.sh**: `scripts/deploy.sh` - Production deployment script
- **provision-server.sh**: `scripts/provision-server.sh` - Server setup
- **Justfile deploy-ec2**: `Justfile:86-88` - EC2 deployment command

### Development
- **dev-workflow.sh**: `scripts/dev-workflow.sh` - Tmux development environment
- **Justfile dev commands**: `Justfile:7-27` - Development quick commands
- **AGENTS.md**: `/AGENTS.md` - Code style and quick reference

### Testing
- **Test commands**: `Justfile:42-53` - Test runners
- **Integration tests**: `sandbox/tests/` - API integration tests
- **E2E testing**: `AGENTS.md:153-181` - agent-browser usage

### Configuration
- **Caddy config**: Referenced in `DEPLOYMENT_RUNBOOK.md:292-357`
- **Systemd services**: `DEPLOYMENT_RUNBOOK.md:360-422`
- **Environment variables**: `DEPLOYMENT_RUNBOOK.md:374-376`

---

## 6. Immediate Recommendations

### For Supervisor Review

1. **Approve runbook taxonomy** - Does the proposed structure meet operational needs?

2. **Prioritize missing runbooks**:
   - P0: Incident Response (operational necessity)
   - P1: Onboarding (agent efficiency)
   - P1: Database Operations (data safety)
   - P2: Monitoring (operational maturity)

3. **Consolidate deployment docs**:
   - Merge `DEPLOYMENT_STRATEGIES`, `DEPLOYMENT_REVIEW`, and `DEPLOYMENT_RUNBOOK`
   - Move from `docs/archive/` to `docs/runbooks/`
   - Create single source of truth

4. **Establish runbook maintenance**:
   - Assign ownership per runbook
   - Set review cycle (quarterly?)
   - Define "last verified" tracking

### Quick Wins (No Approval Needed)

- [ ] Create `docs/runbooks/README.md` as index
- [ ] Add "Last Updated" dates to all runbooks
- [ ] Cross-link AGENTS.md ↔ Justfile ↔ runbooks
- [ ] Archive handoffs older than 30 days to `handoffs/archive/`

---

## 7. Appendix: Document Cross-Reference Matrix

| If you need to... | Read this... | Run this... |
|-------------------|--------------|-------------|
| Deploy to production | `DEPLOYMENT_RUNBOOK.md` | `just deploy-ec2` or `scripts/deploy.sh` |
| Set up new server | `DEPLOYMENT_RUNBOOK.md` §3 | `scripts/provision-server.sh` |
| Start local dev | `AGENTS.md` §Quick Commands | `just dev-sandbox` + `just dev-ui` |
| Run tests | `TESTING_STRATEGY.md` | `just test` |
| Debug production | `DEPLOYMENT_REVIEW.md` §Troubleshooting | `sudo journalctl -u choiros-backend` |
| Continue agent work | `docs/handoffs/README.md` | `python skills/session-handoff/scripts/list_handoffs.py` |
| Understand architecture | `ARCHITECTURE_SPECIFICATION.md` | - |
| Check service status | `AGENTS.md` | `just` (lists all commands) |

---

**Next Steps**: Awaiting supervisor review to proceed with runbook consolidation and creation of missing operational procedures.
