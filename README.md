# ChoirOS — The Automatic Computer

A multi-agent system where autonomous agents collaborate in per-user
isolated sandboxes. Agents manage state, execute tools, and compose
solutions through collective intelligence.

> *Agency lives in computation. Agency exists in language.*

## Getting Started

```bash
# Documentation
cat docs/ATLAS.md                    # auto-generated doc index

# Work graph (via cagent)
cagent work list                     # see all work items
cagent dashboard                     # live supervisor + work status
cagent work ready                    # what's actionable
cagent supervisor                    # start autonomous dispatch

# Development
just dev                             # start local dev (hypervisor + sandbox)
cargo test                           # run tests
cargo fmt --all                      # format
```

## Architecture

```
Hypervisor (control plane)
├── Auth (WebAuthn), Provider Gateway, VM Lifecycle
├── Admin API (token auth, ADR-0020)
│
├── User VMs (per-user sandboxes, ADR-0014)
│   ├── Conductor → Writer → Terminal → cagent
│   ├── EventStore + EventBus (observability)
│   └── Dioxus desktop (WebView)
│
└── Worker VMs (shared pool)
    ├── cagent supervisor + codex/claude adapters
    └── Claims work, executes, attests, reports back
```

## Documentation

Docs use a theory/practice/state/archive taxonomy (ADR-0015):

- [`docs/ATLAS.md`](docs/ATLAS.md) — auto-generated index (pre-commit hook)
- `docs/theory/` — proposed ADRs, design notes, research
- `docs/practice/` — accepted ADRs, implementation guides, runbooks
- `docs/state/` — test reports, snapshots, audits
- `docs/archive/` — historical, superseded

## Work Management

This repo uses [cagent](https://github.com/yusefmosiah/cagent) for work
tracking, attestation, and autonomous agent dispatch. The work graph lives
in `.cagent/cagent.db` (tracked in git). Operational secrets are in
`.cagent/cagent-private.db` (gitignored).

See `CLAUDE.md` for agent instructions.

## Key ADRs

| ADR | Status | Topic |
|-----|--------|-------|
| [0001](docs/practice/decisions/adr-0001-eventstore-eventbus-reconciliation.md) | Deployed | EventStore/EventBus |
| [0007](docs/practice/decisions/adr-0007-3-tier-control-runtime-client-architecture.md) | Deployed | 3-Tier Architecture |
| [0014](docs/theory/decisions/adr-0014-per-user-storage-and-desktop-sync.md) | Phases 1-8 | Per-User VMs |
| [0016](docs/practice/decisions/adr-0016-nixos-declarative-deployment.md) | Deployed | NixOS Deployment |
| [0018](docs/practice/decisions/adr-0018-drop-virtiofs-adaptive-capacity.md) | Deployed | KSM + Virtiofs removal |
| [0020](docs/theory/decisions/adr-0020-security-hardening.md) | Phase 0-1 | Security Hardening |
| [0021](docs/theory/decisions/adr-0021-writer-app-agent-and-collaborative-living-documents.md) | Draft | Writer/Living Docs |
| [0024](docs/theory/decisions/adr-0024-hypervisor-go-rewrite.md) | Proposed | Go Rewrite |
| [0029](docs/theory/decisions/adr-0029-cagent-vsock-work-broker.md) | Proposed | vsock Work Broker |

## Deploy

Push to main → CI auto-deploys to Node B (staging). Promote to Node A via
`promote.yml` workflow. Do not manually SSH-deploy.
