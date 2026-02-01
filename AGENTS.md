# AGENTS.md - ChoirOS Development Guide

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
use actix::{Actor, Addr};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{info, error};

// Internal modules
use crate::actors::{ChatActor, EventStoreActor};
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
- Use `actix::Actor` pattern for concurrent stateful components

## Project Structure

```
choiros-rs/
├── Cargo.toml           # Workspace definition
├── Justfile             # Task runner commands
├── sandbox/             # Backend API + actors
│   ├── src/
│   │   ├── main.rs      # Server entry point
│   │   ├── actors/      # Actix actors (chat, desktop, events)
│   │   ├── api/         # HTTP/WebSocket endpoints
│   │   ├── tools/       # Agent tool system
│   │   └── baml_client/ # LLM integration
│   └── tests/           # Integration tests
├── sandbox-ui/          # Dioxus frontend
│   └── src/
├── shared-types/        # Common types (ActorId, Event, etc.)
├── hypervisor/          # VM management component
└── skills/              # In-repo AI agent skills
    ├── dev-browser/     # Browser automation (Playwright)
    ├── multi-terminal/  # Tmux session management
    └── session-handoff/ # Context preservation
```

## In-Repo Skills

**dev-browser/** - Browser automation testing
- Location: `skills/dev-browser/`
- Purpose: Screenshot testing, web automation, E2E validation
- Usage: `./skills/dev-browser/server.sh &` then run TS scripts
- Key API: `client.page("name")`, `page.screenshot()`, `getAISnapshot()`

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

## Testing Guidelines

**Unit Tests:**
- Place in `src/` files or `tests/` directory
- Use `cargo test --lib` for fast feedback
- Mock external dependencies (DB, network)

**Integration Tests:**
- Place in `tests/*.rs` files
- Use real HTTP requests via `actix_web::test`
- Use temp directories for isolated databases
- Example pattern: `tests/desktop_api_test.rs`

**Running Single Tests:**
```bash
# Run specific test file
cargo test -p sandbox --test desktop_api_test

# Run specific test by name pattern
cargo test -p sandbox test_create_desktop

# Run with output visible
cargo test -p sandbox test_name -- --nocapture
```

## Key Dependencies

- **Async**: tokio, actix, actix-web, futures
- **Web**: dioxus (frontend), actix-ws (WebSocket)
- **Database**: sqlx (SQLite), libsql
- **Serialization**: serde, serde_json
- **IDs**: uuid, ulid
- **Errors**: thiserror, anyhow
- **Tracing**: tracing, tracing-subscriber
- **LLM**: baml

## CI/CD Notes

- All PRs require: formatting check, clippy, tests
- Backend tests run with `cargo test -p sandbox`
- Frontend builds with `cargo build` in `sandbox-ui/`
- E2E tests run on main branch pushes only
