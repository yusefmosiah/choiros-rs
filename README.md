# ChoirOS - The Automatic Computer

**ChoirOS** is the operating system for the **Agent Choir** - a multi-agent system where autonomous agents collaborate in harmony. Each user gets an isolated sandbox where agents (actors) manage state, execute tools, and compose solutions through collective intelligence.

> *Agency lives in computation. Agency exists in language. The Agent Choir sings in the automatic computer.*

## Current Status (2026-03-13)

Human-first docs entrypoint: [`docs/ATLAS.md`](docs/ATLAS.md)

**Working:**
- Supervision-tree runtime (`ApplicationSupervisor -> SessionSupervisor -> {Conductor, Desktop, Terminal, Researcher, Writer}Supervisor`)
- `bash` execution delegated through `TerminalActor` paths rather than direct conductor tool execution
- WebSocket streaming for desktop, terminal, writer, and worker `actor_call` telemetry
- Scope-isolated event retrieval via `session_id` and `thread_id`
- EventBus/EventStore observability backbone with SQLite persistence
- Dioxus 0.7 frontend with DesktopShell, PromptBar, and workspace canvas
- Current model defaults: conductor/writer on `ClaudeBedrockSonnet46`, other callsites on `ClaudeBedrockHaiku45`

**In Progress:**
- Typed worker event schema for actor-call rendering
- Terminal loop event enrichment (`tool_call`, `tool_result`, durations, retry/error metadata)
- Direct worker/app-to-conductor request-message contract
- Ordered websocket integration tests for scoped multi-instance streams
- Writer app-agent harness completion and contract hardening
- Tracing rollout (human UX → headless API → app-agent harness)
- Conductor wake-context with bounded agent-tree snapshots
- Harness simplification (one while-loop model)

## Execution Policy

- Primary orchestration: `Prompt Bar -> Conductor`
- Human interaction: living-document-first (no standalone chat)
- Domain direction: `choir-ip.com` - durable outputs over ephemeral chat
- Model-Led Control Flow: model-managed orchestration; deterministic logic for safety rails only

## Local Start (Canonical)

Use the vfkit cutover path unless you are intentionally doing a direct sandbox debug loop.

```bash
# 1) Build release UI assets
just local-build-ui

# 2) Start local stack (hypervisor + runtime plane)
just dev

# 3) Check status
just dev-status
```

Open:
- `http://127.0.0.1:9090/login` (canonical ingress)

Stop:

```bash
just stop
```

Detailed guide:
- `docs/practice/guides/local-vfkit-nixos-miniguide.md`

## OVH Deploy Path

Use the OVH deployment guide as the canonical operator entrypoint:

- `docs/practice/guides/ovh-config-and-deployment-entrypoint.md`
- `AGENTS.md` contains the current push → pull → build → restart flow and post-deploy verification commands.

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
    ├── ConductorSupervisor
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
├── shared-types/           # Shared Rust types and generated bindings
├── hypervisor/             # Control plane, auth, routing, provider gateway
└── docs/
    ├── ATLAS.md            # Start here
    ├── practice/           # Implemented/current guidance
    ├── theory/             # Proposals and future design
    ├── state/              # Reports and snapshots
    └── archive/            # Historical material
```

## Documentation

- **Entry point:** `docs/ATLAS.md`
- **Dev guide:** `AGENTS.md`
- **Current operating guides:** `docs/practice/guides/`
- **Current accepted decisions:** `docs/practice/decisions/`
- **Future design and planned migrations:** `docs/theory/`
- **Execution evidence and checkpoints:** `docs/state/`
- **Historical context only:** `docs/archive/`

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

## Key Principles

1. **Model-Led Control Flow** - Model plans decomposition; deterministic logic for safety rails only
2. **Actor Messaging** - Control authority via typed actor messages, not string matching
3. **Event Sourcing** - Events are observability transport; typed messages are control flow
4. **Capability Boundaries** - Conductor orchestrates only; workers execute tools
5. **Living Documents** - Human interaction is artifact-first, not ephemeral chat

## License

MIT
