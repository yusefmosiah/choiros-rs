# ChoirOS - The Automatic Computer

**ChoirOS** is the operating system for the **Agent Choir** - a multi-agent system where autonomous agents collaborate in harmony. Each user gets an isolated sandbox where agents (actors) manage state, execute tools, and compose solutions through collective intelligence.

> *Agency lives in computation. Agency exists in language. The Agent Choir sings in the automatic computer.*

## Current Status (2026-02-14)

Human-first docs entrypoint: [`docs/architecture/NARRATIVE_INDEX.md`](docs/architecture/NARRATIVE_INDEX.md)

**Working:**
- Supervision-tree runtime (`ApplicationSupervisor -> SessionSupervisor -> per-type supervisors`)
- Actors: EventStore, EventBus, Desktop, Terminal, Researcher, Conductor, Writer, RunWriter
- Event sourcing with SQLite via sqlx persistence
- WebSocket streaming for desktop, terminal, writer, and telemetry events
- Dioxus 0.7 frontend with DesktopShell, PromptBar, WorkspaceCanvas
- Model providers: AWS Bedrock (Claude), Z.ai (GLM), Kimi

**In Progress:**
- Direct worker/app-to-conductor request-message contract
- Writer app-agent harness hardening
- Tracing rollout (human UX → headless API → app-agent harness)
- Conductor wake-context with bounded agent-tree snapshots
- Harness simplification (one while-loop model)

## Execution Policy

- Primary orchestration: `Prompt Bar -> Conductor`
- Human interaction: living-document-first (no standalone chat)
- Domain direction: `choir-ip.com` - durable outputs over ephemeral chat
- Model-Led Control Flow: model-managed orchestration; deterministic logic for safety rails only

## Quick Start

```bash
# Set local database path
export DATABASE_URL="./data/events.db"

# Build & Run
cargo build -p sandbox
cargo run -p sandbox

# Test
cargo test -p sandbox

# Verify
curl http://localhost:8080/health
```

## Single-Command Grind Host Standup (Nix)

`flake.nix` is now the single source of truth for AWS grind host standup defaults
(AMI, instance type, subnet, security group, key name, and bootstrap repo sync).

```bash
# Provision or resume the grind instance, wait until running,
# then ensure /opt/choiros/workspace is synced to origin/main.
nix run .#standup-grind
```

Optional overrides for different infra values:

```bash
CHOIROS_AWS_REGION=us-east-1 \
CHOIROS_GRIND_NAME=choiros-nixos-grind-01 \
CHOIROS_AMI_ID=ami-xxxxxxxxxxxxxxxxx \
CHOIROS_SUBNET_ID=subnet-xxxxxxxxxxxxxxxxx \
CHOIROS_SECURITY_GROUP_ID=sg-xxxxxxxxxxxxxxxxx \
CHOIROS_SSH_KEY_PATH=~/.ssh/choiros-production.pem \
nix run .#standup-grind
```

## Grind-First DevOps Flow

1. Develop and validate on grind (`/opt/choiros/workspace`) first.
2. Run `just grind-check` from local (or equivalent commands on grind).
3. Commit and push from grind using SSH remote (`git@github.com:yusefmosiah/choiros-rs.git`).
4. Pull locally to stay in sync after push.
5. Build a release manifest on grind (`just release-build-manifest`).
6. Promote exact closures to prod (`just release-promote <grind-host> <prod-host>`).

This avoids rebuild drift between grind and prod by copying the same Nix store paths.

## FlakeHub Cache + Releases

- CI uses FlakeHub Cache via `.github/workflows/nix-ci-draft.yml`.
- Cache is configured explicitly for flake name `choir/choiros-rs`.
- FlakeHub "No available releases" is expected until a publish workflow runs.
- Manual publish workflow is available at `.github/workflows/flakehub-publish.yml`.

For workstation/server cache pulls, authenticate with Determinate Nix:

```bash
# Use a token from https://flakehub.com/user/settings?editview=tokens
determinate-nixd login token --token-file /path/to/flakehub-token.txt
```

Note: FlakeHub Cache write access comes from trusted CI providers (like GitHub Actions),
not ad-hoc pushes from laptops/servers.

## Architecture

```
ApplicationSupervisor
├── EventBusActor
├── EventRelayActor
└── SessionSupervisor
    ├── DesktopSupervisor
    ├── TerminalSupervisor
    ├── ResearcherSupervisor
    └── WriterSupervisor
```

**Runtime Hierarchy (End-State):**
- **Conductor** → orchestrates app agents via typed actor messages
- **App Agents** → run interactive sessions (Writer, etc.)
- **Workers** → concurrent execution (Terminal, Researcher)

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Frontend | Dioxus 0.7 (WASM) |
| Backend | Axum + Ractor |
| Database | SQLite via sqlx |
| LLM | BAML (multi-provider) |

## Project Structure

```
choiros-rs/
├── sandbox/                # Backend API + actors
│   ├── src/actors/         # Conductor, Terminal, Writer, etc.
│   ├── src/api/            # HTTP/WebSocket handlers
│   └── src/supervisor/     # Supervision tree
├── dioxus-desktop/         # Frontend (WASM)
├── shared-types/           # Shared TypeScript/Rust types
├── hypervisor/             # Multi-tenant routing (WIP)
└── docs/
    └── architecture/NARRATIVE_INDEX.md  # Start here
```

## Documentation

- **Entry point:** `docs/architecture/NARRATIVE_INDEX.md`
- **Dev guide:** `AGENTS.md`
- **Platform secrets runbook:** `docs/runbooks/platform-secrets-sops-nix.md`
- **Release flow runbook:** `docs/runbooks/grind-to-prod-release-flow.md`
- **Active handoffs:** `docs/handoffs/` (7 files)
- **Architecture specs:** `docs/architecture/` (47 files)

## Testing

```bash
# Single test file
cargo test -p sandbox --test desktop_api_test -- --nocapture

# Supervision tests
cargo test -p sandbox --features supervision_refactor --test supervision_test -- --nocapture

# Canonical browser E2E (Playwright)
cd tests/playwright
npx playwright test --config=playwright.config.ts

# All tests
cargo test -p sandbox
```

Legacy chat/dev-browser test docs were archived to:
- `docs/archive/testing/legacy-chat/`

## Key Principles

1. **Model-Led Control Flow** - Model plans decomposition; deterministic logic for safety rails only
2. **Actor Messaging** - Control authority via typed actor messages, not string matching
3. **Event Sourcing** - Events are observability transport; typed messages are control flow
4. **Capability Boundaries** - Conductor orchestrates only; workers execute tools
5. **Living Documents** - Human interaction is artifact-first, not ephemeral chat

## License

MIT
