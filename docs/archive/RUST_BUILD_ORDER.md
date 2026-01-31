# ChoirOS Rust Supervisor: Build Order & Early Infrastructure

**Principle:** Do infrastructure setup NOW that prevents refactoring LATER.

---

## Phase 0: Project Skeleton (2 hours)

### 0.1 Rust Toolchain Setup

```bash
# Install Rust (you have it, but ensure latest)
rustup update

# Install essential tools NOW (not later when you need them)
cargo install cargo-nextest      # Faster parallel test runner
cargo install sqlx-cli           # Database migrations
cargo install cargo-deny         # License/security audit
cargo install cargo-watch        # Auto-rebuild on change
cargo install cargo-expand       # Macro debugging
cargo install just               # Command runner (better than Make)
```

**Why now:** These tools change how you write code. Better to have them from day 1.

### 0.2 Project Structure

```bash
mkdir supervisor-rs
cd supervisor-rs
cargo init --name choir-supervisor

# Create directories that prevent mess later
mkdir -p src/{actors,handlers,models,db,baml,utils}
mkdir -p tests/{integration,unit}
mkdir -p .github/workflows
mkdir -p migrations
mkdir -p docs/api
```

### 0.3 Justfile (task runner)

Create `justfile` (like Makefile but better):

```makefile
# Default recipe - show available commands
default:
    @just --list

# Development server with auto-reload
dev:
    cargo watch -x run

# Run all tests (parallel with nextest)
test:
    cargo nextest run

# Run only integration tests
test-integration:
    cargo nextest run --test '*'

# Run with coverage
coverage:
    cargo tarpaulin --out Html

# Database migrations
migrate:
    sqlx migrate run

# Create new migration
new-migration NAME:
    sqlx migrate add {{NAME}}

# Check code quality (CI will run this)
check:
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings
    cargo deny check

# Fix auto-fixable issues
fix:
    cargo fmt
    cargo clippy --fix --allow-staged

# Build release binary
build-release:
    cargo build --release

# Run security audit
audit:
    cargo deny check advisories

# Pre-commit hook (run before git commit)
pre-commit: test check
```

**Why now:** You won't remember all these commands. Scripts prevent "how do I..." later.

---

## Phase 1: Core Infrastructure (4 hours)

### 1.1 Dependencies (Cargo.toml)

Add dependencies in groups to avoid resolution hell:

```toml
[package]
name = "choir-supervisor"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
# Core async
tokio = { version = "1.40", features = ["full", "tracing"] }
actix = "0.13"
actix-rt = "2.10"
futures = "0.3"
async-trait = "0.1"

# Web
actix-web = "4.9"
actix-cors = "0.7"
actix-ws = "0.2"

# Database
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "chrono", "json", "migrate", "uuid"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Time
chrono = { version = "0.4", features = ["serde"] }

# IDs
uuid = { version = "1.11", features = ["v4", "serde"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# Logging/Tracing
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-actix-web = "0.7"

# Configuration
config = "0.14"
dotenvy = "0.15"

# Validation
validator = { version = "0.19", features = ["derive"] }

# Utilities
once_cell = "1.20"
regex = "1.11"

[dev-dependencies]
# Testing
actix-rt = "2.10"
reqwest = { version = "0.12", features = ["json"] }
wiremock = "0.6"

# Assertions
pretty_assertions = "1.4"
insta = { version = "1.42", features = ["yaml", "json"] }  # Snapshot testing
```

**Why now:** Deciding error handling strategy (thiserror vs anyhow) affects every function signature. Do it first.

### 1.2 Configuration Management

Create `src/config.rs` early:

```rust
use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database_url: String,
    pub bind_address: String,
    pub port: u16,
    pub log_level: String,
    pub sandbox_provider: String,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let s = Config::builder()
            .add_source(File::with_name("config/default"))
            .add_source(File::with_name("config/local").required(false))
            .add_source(Environment::with_prefix("CHOIR"))
            .build()?;
        
        s.try_deserialize()
    }
}
```

Create `config/default.toml`:

```toml
bind_address = "127.0.0.1"
port = 8001
log_level = "info"
sandbox_provider = "sprites"
```

**Why now:** Hardcoded values spread like cancer. Config from day 1 prevents tech debt.

---

## Phase 2: Observability (2 hours - DO THIS NOW)

### 2.1 Structured Logging with Tracing

In `main.rs`:

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing FIRST THING
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "choir_supervisor=debug,actix_web=info".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();
    
    tracing::info!("Starting ChoirOS Supervisor");
    
    // ... rest of main
}
```

**Why now:** Adding tracing later means hunting down every println. Do it first and use it everywhere.

### 2.2 Request Tracing Middleware

```rust
use tracing_actix_web::TracingLogger;

App::new()
    .wrap(TracingLogger::default())
    // ... other middleware
```

### 2.3 Structured Spans for Actors

```rust
impl Actor for EventStoreActor {
    type Context = Context<Self>;
    
    fn started(&mut self, _ctx: &mut Self::Context) {
        tracing::info!(actor = "EventStoreActor", "Actor started");
    }
}

impl Handler<AppendEvent> for EventStoreActor {
    type Result = Result<Event, EventStoreError>;
    
    fn handle(&mut self, msg: AppendEvent, _ctx: &mut Self::Context) -> Self::Result {
        let span = tracing::info_span!("append_event", event_type = %msg.event_type);
        let _enter = span.enter();
        
        tracing::debug!("Appending event to store");
        // ... logic
        
        tracing::info!(event_id = %event.id, "Event appended");
        Ok(event)
    }
}
```

**Why now:** Debugging actor systems without tracing is hell. Spans show you the full request flow across actors.

---

## Phase 3: Database Setup (2 hours)

### 3.1 SQLx Migrations (DO NOT SKIP)

```bash
# Create first migration
sqlx migrate add create_event_table
```

This creates `migrations/20260130000000_create_event_table.sql`:

```sql
CREATE TABLE IF NOT EXISTS events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    type TEXT NOT NULL,
    payload JSON NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'local'
);

CREATE INDEX idx_events_type ON events(type);
CREATE INDEX idx_events_timestamp ON events(timestamp);
CREATE INDEX idx_events_user ON events(user_id);
```

**Why now:** Manual schema changes become nightmares. Migrations are version control for DB.

### 3.2 Compile-Time SQL Checking

Use `sqlx::query!` macro (checks SQL at compile time):

```rust
let event = sqlx::query!(
    r#"
    INSERT INTO events (event_id, timestamp, type, payload, user_id)
    VALUES (?1, ?2, ?3, ?4, ?5)
    RETURNING seq, event_id, timestamp, type as "type!", payload, user_id
    "#,
    event_id,
    timestamp,
    event_type,
    payload,
    user_id
)
.fetch_one(&self.pool)
.await?;
```

**Why now:** Runtime SQL errors are the worst. SQLx catches typos at compile time.

---

## Phase 4: Error Handling Strategy (1 hour)

### 4.1 Define Error Types Early

Create `src/errors.rs`:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChoirError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("Actor mailbox closed: {0}")]
    ActorMailbox(String),
    
    #[error("Event not found: {0}")]
    EventNotFound(i64),
    
    #[error("Invalid event type: {0}")]
    InvalidEventType(String),
    
    #[error("Sandbox error: {0}")]
    Sandbox(String),
    
    #[error("BAML/LLM error: {0}")]
    LLM(String),
}

pub type ChoirResult<T> = Result<T, ChoirError>;

// Convert to Actix error for HTTP responses
impl actix_web::error::ResponseError for ChoirError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ChoirError::EventNotFound(_) => HttpResponse::NotFound().finish(),
            ChoirError::InvalidEventType(_) => HttpResponse::BadRequest().finish(),
            _ => HttpResponse::InternalServerError().finish(),
        }
    }
}
```

**Why now:** Error handling strategy affects every function. Decide once, use everywhere.

---

## Phase 5: Testing Infrastructure (3 hours)

### 5.1 Test Utilities

Create `tests/common/mod.rs`:

```rust
use sqlx::SqlitePool;
use choir_supervisor::db::EventStoreActor;

pub async fn setup_test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    
    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .unwrap();
    
    pool
}

pub async fn setup_test_event_store() -> EventStoreActor {
    let pool = setup_test_db().await;
    EventStoreActor::new(pool)
}
```

### 5.2 First Integration Test

Create `tests/event_store_integration.rs`:

```rust
use choir_supervisor::db::{EventStoreActor, AppendEvent};
use actix::Actor;

mod common;

#[actix::test]
async fn test_event_store_append_and_retrieve() {
    // Setup
    let store = common::setup_test_event_store().await.start();
    
    // Test
    let event = store
        .send(AppendEvent {
            event_type: "test.event".to_string(),
            payload: serde_json::json!({"foo": "bar"}),
            source: "test".to_string(),
        })
        .await
        .unwrap()
        .unwrap();
    
    // Assert
    assert_eq!(event.event_type, "test.event");
    assert!(event.seq > 0);
}
```

**Why now:** Writing tests after the fact is 10x harder. Test infrastructure from day 1.

### 5.3 Snapshot Testing for Complex Types

```rust
use insta::assert_json_snapshot;

#[test]
fn test_event_serialization() {
    let event = Event {
        seq: 1,
        event_type: "user.msg".to_string(),
        payload: json!({"content": "hello"}),
        // ...
    };
    
    assert_json_snapshot!(event);
}
```

**Why now:** Complex JSON structures change. Snapshots catch unintended changes.

---

## Phase 6: Health Checks & Observability (1 hour)

### 6.1 Health Endpoint

```rust
use actix_web::{HttpResponse, Responder};
use sqlx::SqlitePool;

pub async fn health(pool: web::Data<SqlitePool>) -> impl Responder {
    // Check database
    let db_healthy = sqlx::query("SELECT 1")
        .fetch_one(pool.get_ref())
        .await
        .is_ok();
    
    if db_healthy {
        HttpResponse::Ok().json(serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION"),
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }))
    } else {
        HttpResponse::ServiceUnavailable().json(serde_json::json!({
            "status": "unhealthy",
            "reason": "database connection failed"
        }))
    }
}
```

### 6.2 Metrics Endpoint (Prometheus format)

```rust
pub async fn metrics() -> impl Responder {
    // Basic process metrics
    HttpResponse::Ok().body(format!(
        "# HELP choir_build_info Build information\n\
         # TYPE choir_build_info gauge\n\
         choir_build_info{{version=\"{}\"}} 1\n",
        env!("CARGO_PKG_VERSION")
    ))
}
```

**Why now:** Monitoring in prod without health checks is flying blind. Add early.

---

## Phase 7: CI/CD Pipeline (2 hours)

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      
      - name: Cache cargo
        uses: Swatinem/rust-cache@v2
      
      - name: Install sqlx-cli
        run: cargo install sqlx-cli --no-default-features --features native-tls,sqlite
      
      - name: Check formatting
        run: cargo fmt -- --check
      
      - name: Run clippy
        run: cargo clippy --all-targets --all-features -- -D warnings
      
      - name: Run tests
        run: cargo test --all-features
      
      - name: Check SQLx queries
        run: cargo sqlx prepare --check
        env:
          DATABASE_URL: sqlite::memory:
```

**Why now:** CI from day 1 prevents "works on my machine". SQLx prepare check ensures queries are cached.

---

## Build Order Summary

**DO IN THIS ORDER:**

1. **Tools** (30 min) - cargo-nextest, sqlx-cli, just
2. **Config** (30 min) - Settings struct, env vars, toml files
3. **Tracing** (1 hour) - Structured logging, spans, middleware
4. **Database** (2 hours) - Migrations, connection pool, compile-time SQL
5. **Errors** (30 min) - ChoirError enum, ResponseError impl
6. **Tests** (2 hours) - Test utils, first integration test, snapshots
7. **Health** (30 min) - /health, /metrics endpoints
8. **CI** (2 hours) - GitHub Actions, caching, sqlx prepare check

**THEN:**

9. First actor (EventStore)
10. First handler (/health)
11. Second actor (ChatActor)
12. WebSocket handler
13. More actors...

**What this prevents:**

- "How do I run tests again?" → Justfile
- "Why is this query failing in prod?" → SQLx compile-time checks
- "What happened in production?" → Structured tracing
- "Where do I put config?" → Settings module
- "Is the service healthy?" → Health endpoint
- "CI is slow" → Caching
- "Database schema drift" → Migrations

**Ready to start with Phase 0?**