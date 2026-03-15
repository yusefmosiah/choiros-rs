Date: 2026-03-15
Kind: Architecture audit
Status: Complete
Tool: cagent mind-graph + Claude Opus 4.6
Method: Parallel subagent analysis (4 agents: theory, practice, state, archive)

## Documentation Landscape Audit

Full corpus analysis of choiros-rs documentation for cagent work graph ingestion.

### Corpus Summary

| Layer | Count | Purpose |
|-------|-------|---------|
| Theory ADRs | 16 | Planned architecture decisions (Go rewrite, memory, publishing, scaling, security) |
| Practice ADRs | 9 | Implemented/operational architecture (NixOS, systemd VMs, virtiofs removal, concurrency) |
| Theory guides | 7 | Implementation plans for theory ADRs |
| Practice guides | 19 | Operational runbooks, API contracts, deployment guides |
| State reports | 15 | Load tests, security audits, stress tests, E2E results |
| State snapshots | 5 | Point-in-time checkpoints |
| Theory notes | 9 | Research, inbox, deferred items |
| Archive | 217 | Project history (Jan 29 - Mar 9, 2026) |
| **Total** | **297** | |

### Doc Ontology

The project uses a theory/practice/state/archive taxonomy:
- **theory/decisions/**: ADRs for planned work (draft/proposed)
- **practice/decisions/**: ADRs for implemented work (accepted/deployed)
- **theory/guides/**: Implementation plans paired with theory ADRs
- **practice/guides/**: Operational runbooks for implemented systems
- **state/reports/**: Test results, audits, feasibility studies (attestation evidence)
- **state/snapshots/**: Point-in-time session handoffs and checkpoints
- **theory/notes/**: Research, inbox items, deferred work
- **archive/**: Historical handoffs, deprecated docs, superseded designs

### ADR Dependency Graph

Critical hubs:
- **ADR-0014** (per-user VMs) — feeds into security (0020), scaling (0028), Go rewrite (0024), publishing (0027)
- **ADR-0024** (Go rewrite) — unblocks admin dashboard (0025), agent dispatch (0026), bootstrap proof

```
ADR-0014 (Per-User VM Lifecycle) [CRITICAL HUB]
  ├─ feeds → ADR-0020 (Security Hardening)
  ├─ feeds → ADR-0024 (Go Rewrite)
  ├─ feeds → ADR-0028 (Multi-Provider Scaling)
  └─ feeds → ADR-0011 (State/Compute Decoupling)

ADR-0024 (Go Rewrite) [CRITICAL MILESTONE]
  ├─ feeds → ADR-0025 (Go Admin Dashboard)
  ├─ feeds → ADR-0026 (Self-Directing Agent Dispatch)
  └─ requires → ADR-0014, ADR-0021

ADR-0016 (NixOS Declarative) → ADR-0017 (systemd VMs) → ADR-0018 (drop virtiofs) → ADR-0022 (concurrency)

ADR-0019 (Per-User Memory) → ADR-0027 (Publishing) → ADR-0011 (Bootstrap Into Publishing)
ADR-0021 (Writer App Agent) → ADR-0026 (Self-Directing Dispatch)
```

### Theory ADRs (16) — Planned/Proposed Work

| ADR | Title | Status | Priority |
|-----|-------|--------|----------|
| 0002 | Rust + Nix Build and Cache Strategy | Draft | 5 |
| 0003 | Hypervisor-Sandbox Secrets Boundary | Draft | 4 |
| 0004 | Hypervisor-Sandbox UI Runtime Boundary | Proposed | 4 |
| 0009 | Terminal Renderer Strategy | Proposed | 5 |
| 0011 | Bootstrap Into Publishing | Proposed | 3 |
| 0013 | Fleet-Ctl Change Lifecycle | Draft | 3 |
| 0014 | Per-User VM Lifecycle and Storage | Draft | 2 |
| 0019 | Per-User Memory Curation | Draft | 4 |
| 0020 | Security Hardening | Accepted | 1 |
| 0021 | Writer App Agent and Living Documents | Draft | 2 |
| 0023 | microvm.nix Store Disk Transport | Proposed | 1 |
| 0024 | Go Rewrite — Hypervisor Decomposition | Proposed | 2 |
| 0025 | Go Admin Dashboard | Proposed | 1 |
| 0026 | Self-Directing Agent Dispatch | Proposed | 2 |
| 0027 | Publishing and Global Knowledge Base | Draft | 3 |
| 0028 | Multi-Provider LLM Scaling | Draft | 2 |

### Practice ADRs (9) — Implemented/Operational

| ADR | Title | Status | Has Guide |
|-----|-------|--------|-----------|
| 0001 | EventStore/EventBus Reconciliation | Accepted | No |
| 0007 | 3-Tier Control/Runtime/Client | Accepted | No |
| 0008 | OVH Self-Hosted Secrets | Accepted | Yes (platform-secrets) |
| 0012 | OVH Bootstrap Secrets and Compute | Accepted | Yes (ovh-config) |
| 0015 | Documentation Kanban Architecture | Draft | Yes (docs-system) |
| 0016 | NixOS Declarative Deployment | Accepted | Yes |
| 0017 | systemd-Native VM Lifecycle | Accepted | Yes |
| 0018 | Drop Virtiofs, KSM, Adaptive Capacity | Accepted | Yes |
| 0022 | Hypervisor Concurrency and Capacity | Accepted | Yes |

### Attestation Evidence (15 Reports)

| Report | Date | Type | Relates to |
|--------|------|------|-----------|
| Provider Matrix (Kimi) | 2026-02-26 | Validation | Provider gateway |
| Provider Matrix (All Models) | 2026-02-26 | Validation | Provider gateway |
| Local Cutover Step 1 | 2026-02-26 | Integration | CI/CD |
| Per-User VM Load Test | 2026-03-09 | Load test | ADR-0014 |
| Heterogeneous Load Test | 2026-03-09 | Load test | ADR-0014 |
| ADR-0018 Load Test | 2026-03-10 | Validation | ADR-0018 |
| ADR-0022 Concurrency Stress | 2026-03-11 | Stress test | ADR-0022 |
| Capacity Stress Test | 2026-03-11 | Stress test | ADR-0014, ADR-0018 |
| DAX vs No-DAX Load Test | 2026-03-11 | Comparison | ADR-0023 |
| Heterogeneous Workload Stress | 2026-03-11 | Stress test | Topology |
| Machine Class Stress Comparison | 2026-03-11 | Matrix test | ADR-0014 |
| Secrets Architecture Audit | 2026-03-12 | Security audit | ADR-0003, ADR-0020 |
| Conductor E2E Intelligence | 2026-02-10 | E2E test | Agent architecture |
| Go Refactor Feasibility | 2026-03-09 | Feasibility | ADR-0024 |
| Node B E2E Report | 2026-03-08 | Integration | Hypervisor |

### Mapping to cagent Work Graph

The doc ontology maps directly to cagent work item states:

| Doc Type | cagent Concept | State |
|----------|---------------|-------|
| Theory ADR | Work item | draft or ready |
| Practice ADR | Work item | done |
| Implementation guide | Reachability attestation | linked to ADR work item |
| Test report | Attestation record | evidence attached to work item |
| Snapshot | Work note | point-in-time observation |
| Archive | Compacted note | historical context |

### Project History Arc

Four phases identified from the 217 archived docs:

1. **Vision & Architecture** (Jan 29 - Feb 1): Actor model, event sourcing, multi-agent vision
2. **Foundation Build** (Feb 1 - Feb 14): Actix→Ractor migration, conductor/writer emergence
3. **Cloud Deployment** (Feb 15 - Feb 28): AWS→OVH pivot, NixOS, local-first philosophy
4. **Platform Reset** (Mar 1 - Mar 12): ADR model, per-user VMs, stress testing, Go rewrite planning

### Key Metrics from Evidence

- **VM capacity:** 60 active VMs (proxy-limited), 300+ idle (memory-limited with DAX)
- **Per-VM footprint:** ~407-514 MB (varies by config)
- **KSM benefit:** 3.5x improvement with DAX (105 vs 363 MB Pss)
- **Snapshot restore:** 4.5s (2.68x faster than cold boot)
- **E2E coverage:** 76% pass rate (31/41 tests)
- **Conductor:** 6/6 E2E scenarios pass, handles 7 decision types
