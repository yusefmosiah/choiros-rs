# AGENTS.md - ChoirOS Development Guide

## Current Architecture Snapshot (2026-02-07)

- Runtime is supervision-tree-first:
  - `ApplicationSupervisor -> SessionSupervisor -> {ChatSupervisor, TerminalSupervisor, DesktopSupervisor}`
- `bash` tool execution is delegated through TerminalActor paths (no direct ChatAgent shell execution).
- Terminal work emits worker lifecycle + progress telemetry and streams as websocket `actor_call` chunks.
- Scope isolation (`session_id`, `thread_id`) is required for chat/tool event retrieval to prevent cross-instance bleed.
- EventBus/EventStore are the observability backbone for worker/task tracing.

## Current High-Priority Development Targets

1. Typed worker event schema for actor-call rendering (`spawned/progress/complete/failed`).
2. Terminal loop event enrichment (`tool_call`, `tool_result`, durations, retry/error metadata).
3. WatcherActor prototype for timeout/failure escalation signals to supervisors.
4. Ordered websocket integration tests for scoped multi-instance streams.

## Quick Commands

```bash
# Development
just dev-sandbox     # Run backend API server
just dev-ui          # Run frontend dev server (port 3000)
just dev-hypervisor  # Run hypervisor component

# Building
just build           # Build all packages in release mode
just build-sandbox   # Build frontend + backend for production

# Testing
just test            # Run all tests across workspace
just test-unit       # Run unit tests only (--lib)
just test-integration # Run integration tests (--test '*')
cargo test -p sandbox --test desktop_api_test  # Run single test file
cargo test -p sandbox test_name_pattern       # Run specific test

# Code Quality
just check           # Check formatting + clippy
just fix             # Auto-fix formatting and clippy issues
cargo fmt --check    # Check formatting only
cargo clippy --workspace -- -D warnings  # Run clippy

# Database
just migrate         # Run SQLx migrations
just new-migration NAME  # Create new migration

# Docker
just docker-build    # Build Docker image
just docker-run      # Run Docker container

# Deployment
just deploy-ec2      # Deploy to EC2 instance
```

## Code Style Guidelines

### Rust Standards

**Formatting:**
- Use `cargo fmt` (enforced in CI)
- Max line length: 100 characters (default)
- 4 spaces for indentation
- Trailing commas in multi-line structs/enums

**Imports:**
```rust
// Standard library first
use std::collections::HashMap;
use std::sync::Arc;

// External crates (alphabetical within groups)
use chrono::{DateTime, Utc};
use ractor::{concurrency::Duration, Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, error};

// Internal modules
use crate::actors::{ChatActor, EventStoreActor, TerminalActor};
use shared_types::{ActorId, Event};
```

**Naming Conventions:**
- Types (structs, enums, traits): `PascalCase` - `ChatActor`, `EventStore`
- Functions, variables, modules: `snake_case` - `get_actor`, `event_store`
- Constants, statics: `SCREAMING_SNAKE_CASE` - `MAX_EVENTS`, `DEFAULT_TIMEOUT`
- Acronyms: Treat as words - `ActorId`, `HttpRequest` (not `ActorID`, `HTTPRequest`)

**Documentation:**
```rust
//! Module-level documentation starts with //!

/// Struct/function documentation with triple slash
/// 
/// # Examples
/// ```
/// let id = ActorId::new();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorId(pub String);
```

**Error Handling:**
- Use `thiserror` for custom error types
- Use `anyhow` for application-level error propagation
- Prefer `Result<T, E>` over panics
- Log errors with context using `tracing::error!`

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ActorError {
    #[error("Actor not found: {0}")]
    NotFound(String),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

**Types & Generics:**
- Prefer explicit types over `impl Trait` in public APIs
- Use `Option<T>` and `Result<T, E>` consistently
- Leverage workspace dependencies from `Cargo.toml`

**Async/Await:**
- Use `tokio` runtime (configured in workspace)
- Prefer `async fn` over raw futures
- Use `ractor::Actor` pattern for concurrent stateful components

## Project Structure

```
choiros-rs/
├── Cargo.toml           # Workspace definition
├── Justfile             # Task runner commands
├── sandbox/             # Backend API + actors
│   ├── src/
│   │   ├── main.rs      # Server entry point
│   │   ├── actors/      # Ractor actors (chat, desktop, events, terminal)
│   │   ├── api/         # HTTP/WebSocket endpoints
│   │   ├── tools/       # Agent tool system
│   │   └── baml_client/ # LLM integration
│   └── tests/           # Integration tests
├── dioxus-desktop/      # Dioxus 0.7 frontend (WASM)
├── shared-types/        # Common types (ActorId, Event, etc.)
├── hypervisor/          # VM management component
└── skills/              # In-repo AI agent skills
    ├── multi-terminal/  # Tmux session management
    └── session-handoff/ # Context preservation
```

## In-Repo Skills

**multi-terminal/** - Terminal session management  
- Location: `skills/multi-terminal/`
- Purpose: Orchestrate multiple processes (servers, tests, logs)
- Usage: `python skills/multi-terminal/scripts/terminal_session.py`
- Key API: `TerminalSession("name")`, `add_window()`, `capture_output()`

**session-handoff/** - Context preservation
- Location: `skills/session-handoff/`
- Purpose: Create handoff docs for multi-session agent workflows
- Usage: `python skills/session-handoff/scripts/create_handoff.py task-name`
- Creates: `docs/handoffs/YYYY-MM-DD-task-name.md`

## E2E Testing with agent-browser

**agent-browser** - CLI-based browser automation (installed globally)
- Purpose: Screenshot testing, web automation, E2E validation
- Install: `npx skills add vercel-labs/agent-browser@agent-browser -g`
- Key Commands:
  - `agent-browser open <url>` - Navigate to page
  - `agent-browser snapshot -i` - Get interactive elements with refs
  - `agent-browser click @e1` - Click element by ref
  - `agent-browser screenshot` - Take screenshot
  - `agent-browser --json` - JSON output for programmatic use

**Example E2E Test Flow:**
```bash
# 1. Start services (in separate terminals or tmux)
just dev-sandbox    # Backend on port 8080
just dev-ui         # Frontend on port 3000

# 2. Run E2E test with agent-browser
agent-browser open http://localhost:3000
agent-browser screenshot tests/screenshots/initial.png
agent-browser snapshot -i
# Use refs from snapshot to interact
agent-browser click @e1
agent-browser fill @e2 "test message"
agent-browser click @e3
agent-browser wait --text "AI response"
agent-browser screenshot tests/screenshots/result.png
```

## Testing Guidelines

**Unit Tests:**
- Place in `src/` files or `tests/` directory
- Use `cargo test --lib` for fast feedback
- Mock external dependencies (DB, network)

**Integration Tests:**
- Place in `tests/*.rs` files
- Use Axum router + `tower::ServiceExt::oneshot`
- Use temp directories for isolated databases
- Example pattern: `tests/desktop_api_test.rs`
- For websocket chat flows, prefer `tokio_tungstenite` integration tests over manual curl loops.
- Assert `actor_call` chunks for delegated terminal tasks when validating multi-agent observability.
- Keep tests provider-agnostic: do not hardcode assumptions to a single external weather/API service.

**Running Single Tests:**
```bash
# Run specific test file
cargo test -p sandbox --test desktop_api_test

# Run specific test by name pattern
cargo test -p sandbox test_create_desktop

# Run with output visible
cargo test -p sandbox test_name -- --nocapture

# Run websocket chat integration suite
cargo test -p sandbox --test websocket_chat_test -- --nocapture

# Run supervision delegation suite
cargo test -p sandbox --features supervision_refactor --test supervision_test -- --nocapture
```

## Key Dependencies

- **Async**: tokio, futures
- **Web**: axum, tower, tower-http, dioxus (frontend)
- **Database**: sqlx (SQLite), libsql
- **Serialization**: serde, serde_json
- **IDs**: uuid, ulid
- **Errors**: thiserror, anyhow
- **Tracing**: tracing, tracing-subscriber
- **LLM**: baml
- **Dev Tools**: cargo-watch (auto-rebuild), multitail (log monitoring)

## Task Concurrency

**CRITICAL: This section defines the automatic computer architecture.**

### Core Principles

1. **Supervisors NEVER spawn blocking task() calls**
   - Supervisors coordinate; they don't execute
   - A supervisor waiting on a task blocks the entire workflow

2. **Supervisors coordinate, workers execute**
   - Supervisors: Plan, delegate, aggregate results
   - Workers: Perform actual work, report back

3. **Use OpenCode subagent tasks for parallel work (interim)**
   - Fire-and-forget async execution via OpenCode SDK
   - Workers run independently without blocking supervisor
   - Note: Native actor messaging coming to ChoirOS (TerminalActor, EventBus)

4. **Tool call budgets:**
   - Supervisor: 50 calls (coordination only)
   - Worker: 200 calls (execution work)
   - Async Run: Unlimited (true parallelism)

### Correct vs Wrong Patterns

**WRONG: Supervisor blocks on worker task**
```python
# DON'T DO THIS - blocks supervisor
result = task("Analyze file", prompt="...")  # Blocking!
```

**CORRECT: Supervisor delegates via async run**
```python
# DO THIS - non-blocking coordination
run_async(
    agent="file-analyzer",
    prompt="Analyze file: " + filepath,
    on_complete="handle_analysis_result"
)
# Supervisor continues immediately, doesn't wait
```

**WRONG: Sequential blocking in loop**
```python
# DON'T DO THIS - serial blocking
results = []
for file in files:
    result = task(f"Process {file}", ...)  # Blocks each iteration
    results.append(result)
```

**CORRECT: Parallel async execution**
```python
# DO THIS - parallel execution
for file in files:
    run_async(
        agent="file-processor",
        prompt=f"Process: {file}",
        on_complete="collect_result"
    )
# All files processed in parallel, supervisor moves on
```

**WRONG: Worker trying to coordinate**
```python
# DON'T DO THIS - workers shouldn't spawn sub-workers
# In worker context:
result = task("Sub-task", ...)  # Wrong! Workers execute, don't delegate
```

**CORRECT: Worker completes its work**
```python
# DO THIS - worker does its assigned work
analysis = perform_analysis(data)
return {"status": "complete", "result": analysis}
```

### Architecture Summary

```
Supervisor (50 calls)
    ├── run_async(worker1) ──┐
    ├── run_async(worker2) ──┼── Parallel execution
    ├── run_async(worker3) ──┘
    └── Continue coordinating...

Worker (200 calls)
    └── Execute task → Return result
```

**Remember:** Blocking is the enemy of scalability. Use async runs for true parallelism.

## CI/CD Notes

- All PRs require: formatting check, clippy, tests
- Backend tests run with `cargo test -p sandbox`
- Frontend builds with `dx build` in `dioxus-desktop/`
- E2E tests run on main branch pushes only
