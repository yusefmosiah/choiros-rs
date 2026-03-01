# ChoirOS Supervision Implementation Plan

## Executive Summary

> Status update (2026-02-06 cutover): runtime has been moved to supervision-first
> request paths and API handlers no longer depend on `ActorManager`. Remaining
> content in this document should be treated as historical migration context.

This document provides a comprehensive, actionable plan to migrate ChoirOS from the current `ActorManager`-based architecture (which uses DashMap and Mutex anti-patterns) to a proper ractor supervision tree. This migration will provide automatic fault recovery, eliminate race conditions, and align with Erlang/OTP best practices.

**Current State:** Unsupervised actors managed via DashMap with Mutex-based coordination
**Target State:** Hierarchical supervision tree with automatic restarts and registry-based discovery
**Estimated Effort:** 6 weeks (5 phases + 1 week buffer)
**Risk Level:** Medium-High (requires careful testing due to actor lifecycle changes)

---

## 1. Current State Analysis

### 1.1 Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│  ActorManager (DashMap + Mutex) - ANTI-PATTERN        │
│  ┌────────────┬─────────────┬─────────────┐│
│  │chat_actors  │chat_agents  │desktop_actors││
│  │DashMap     │DashMap     │DashMap     ││
│  └────────────┴─────────────┴─────────────┘│
│       ▲                                           │
│       └─ Direct Actor::spawn (unsupervised)     │
└─────────────────────────────────────────────────────────────┘
              │
    ┌─────────┼─────────┐
    ▼         ▼         ▼
ChatActor  DesktopActor  TerminalActor
```

### 1.2 Anti-Patterns Identified

| Location | Anti-Pattern | Impact |
|----------|---------------|---------|
| `actor_manager.rs:31-34` | DashMap for actor registry | Stale refs after crash, manual cleanup required |
| `actor_manager.rs:35` | Mutex for terminal creation | Global bottleneck, deadlock potential |
| `actor_manager.rs:71-81` | Direct `Actor::spawn` without supervision | No automatic restart on failure |
| `actor_manager.rs:207-211` | Manual `remove_terminal` cleanup | Races with supervision, inconsistent state |
| All actor files | No `SupervisionEvent` handling | No fault recovery or restart logic |

### 1.3 Root Cause

The current implementation treats ractor as a thread-pool with message passing, rather than embracing the actor model's supervision philosophy. External concurrency primitives (DashMap, Mutex) bypass ractor's isolation guarantees and prevent proper fault containment.

---

## 2. Target Architecture

### 2.1 Supervision Tree Design

```
┌───────────────────────────────────────────────────────────────┐
│  ApplicationSupervisor (Root)                                 │
│  Strategy: rest_for_one (cascading)                          │
│  ┌────────────────────────────────────────────────────┐   │
│  │ SessionSupervisor                                  │   │
│  │ Strategy: one_for_one (isolated restarts)         │   │
│  │ ┌──────────────┬──────────────┬─────────────────┐  │   │
│  │ │DesktopSup    │ ChatSup      │ TerminalSup     │  │   │
│  │ │one_for_one   │ one_for_one  │ simple_one_for_ │  │   │
│  │ │              │              │ one (dynamic)   │  │   │
│  │ └──────────────┴──────────────┴─────────────────┘  │   │
│  └────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────┘
         │                │                │
         ▼                ▼                ▼
   DesktopActor     ChatActor      TerminalFactory
   (named)          (named)        ├─ TerminalWorker
   registry:        registry:       ├─ TerminalWorker
   "desktop:{id}"   "chat:{id}"     └─ TerminalWorker
```

### 2.2 Component Responsibilities

| Component | Strategy | Responsibility | Restart Policy |
|-----------|----------|----------------|----------------|
| **ApplicationSupervisor** | `rest_for_one` | Root supervisor, spawns domain supervisors | Escalate to system level |
| **SessionSupervisor** | `one_for_one` | Manages domain supervisors | Restart failed domain |
| **DesktopSupervisor** | `one_for_one` | Spawns per-user DesktopActors | Max 3 restarts in 60s |
| **ChatSupervisor** | `one_for_one` | Spawns per-chat ChatActors/ChatAgents | Max 5 restarts in 30s |
| **TerminalSupervisor** | `simple_one_for_one` | Manages TerminalFactory | Factory restarts workers |
| **TerminalFactory** | N/A (factory) | Worker pool for terminals | Auto-restart via factory |

### 2.3 Actor Naming Convention

| Actor Type | Registry Name Format | Example |
|------------|---------------------|---------|
| DesktopActor | `desktop:{desktop_id}` | `desktop:user-123-session-456` |
| ChatActor | `chat:{actor_id}` | `chat:thread-789` |
| ChatAgent | `agent:{agent_id}` | `agent:assistant-456` |
| TerminalFactory | `terminal_factory:{user_id}` | `terminal_factory:user-123` |
| TerminalWorker | `terminal:{terminal_id}` | `terminal:term-xyz` |

---

## 3. Phase-by-Phase Migration Plan

### Phase 1: Foundation - Week 1
**Goal:** Establish supervision tree skeleton without disrupting existing APIs
**Risk:** Low

#### Tasks

1. **Create supervisor module structure** (`src/supervisors/`)
   ```
   src/supervisors/
   ├── mod.rs
   ├── application_supervisor.rs
   ├── session_supervisor.rs
   ├── desktop_supervisor.rs
   ├── chat_supervisor.rs
   └── terminal_supervisor.rs
   ```

2. **Implement ApplicationSupervisor** (`src/supervisors/application_supervisor.rs`)
   - Root supervisor with `rest_for_one` strategy
   - Handles domain supervisor failures
   - Feature flag `supervision_refactor` (disabled by default)

3. **Implement SessionSupervisor** (`src/supervisors/session_supervisor.rs`)
   - Middle layer with `one_for_one` strategy
   - Spawns domain supervisors as children
   - Tracks domain supervisor ActorIds

4. **Add feature flag to Cargo.toml**
   ```toml
   [features]
   default = []
   supervision_refactor = []
   ```

5. **Wire supervision tree in main.rs** (behind feature flag)
   - Spawn ApplicationSupervisor before EventStoreActor
   - Pass supervisor reference to AppState

#### Code Example: ApplicationSupervisor

```rust
// src/supervisors/application_supervisor.rs

use ractor::{Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ApplicationSupervisor;

pub struct ApplicationState {
    domain_supervisors: HashMap<ractor::ActorId, DomainType>,
    event_store: ActorRef<crate::actors::EventStoreMsg>,
}

#[derive(Debug, Clone)]
pub enum DomainType {
    Session,
}

#[derive(Debug)]
pub enum ApplicationSupervisorMsg {
    GetSessionSupervisor { reply: ractor::RpcReplyPort<ActorRef<SessionSupervisorMsg>> },
    Supervision(SupervisionEvent),
}

#[ractor::async_trait]
impl Actor for ApplicationSupervisor {
    type Msg = ApplicationSupervisorMsg;
    type State = ApplicationState;
    type Arguments = ActorRef<crate::actors::EventStoreMsg>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        event_store: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            supervisor_id = %myself.get_id(),
            "ApplicationSupervisor starting"
        );

        // Spawn SessionSupervisor as child (linked via supervision)
        let (session_sup, session_handle) = Actor::spawn(
            Some("session_supervisor".to_string()),
            crate::supervisors::SessionSupervisor,
            event_store.clone(),
        ).await?;

        // SessionSupervisor will be supervised via handle
        tokio::spawn(async move {
            if let Err(e) = session_handle.await {
                tracing::error!("SessionSupervisor exited with error: {}", e);
            }
        });

        let mut domain_supervisors = HashMap::new();
        domain_supervisors.insert(session_sup.get_id(), DomainType::Session);

        Ok(ApplicationState {
            domain_supervisors,
            event_store,
        })
    }

    async fn handle_supervision_event(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match event {
            SupervisionEvent::ActorFailed(actor_cell, error) => {
                tracing::error!(
                    supervisor_id = %myself.get_id(),
                    failed_actor = %actor_cell.get_id(),
                    error = %error,
                    "Domain supervisor failed - restarting"
                );

                // Restart failed domain supervisor
                if let Some(domain_type) = state.domain_supervisors.remove(&actor_cell.get_id()) {
                    match domain_type {
                        DomainType::Session => {
                            let (new_sup, handle) = Actor::spawn(
                                Some("session_supervisor".to_string()),
                                crate::supervisors::SessionSupervisor,
                                state.event_store.clone(),
                            ).await?;
                            
                            state.domain_supervisors.insert(new_sup.get_id(), DomainType::Session);
                            
                            tokio::spawn(async move {
                                let _ = handle.await;
                            });
                        }
                    }
                }
            }
            SupervisionEvent::ActorTerminated(actor_cell, _, _) => {
                state.domain_supervisors.remove(&actor_cell.get_id());
                tracing::info!(
                    actor_id = %actor_cell.get_id(),
                    "Domain supervisor terminated"
                );
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ApplicationSupervisorMsg::GetSessionSupervisor { reply } => {
                // Find SessionSupervisor by name
                if let Some(cell) = ractor::registry::where_is("session_supervisor".to_string()) {
                    let _ = reply.send(cell.into());
                } else {
                    tracing::error!("SessionSupervisor not found in registry");
                }
            }
            ApplicationSupervisorMsg::Supervision(event) => {
                // Handled by handle_supervision_event
            }
        }
        Ok(())
    }
}
```

#### Testing Strategy

```rust
// tests/supervision_phase1_test.rs

#[tokio::test]
async fn test_application_supervisor_spawns_session_supervisor() {
    let (event_store, _) = Actor::spawn(
        None,
        EventStoreActor,
        EventStoreArguments::InMemory,
    ).await.unwrap();

    let (app_sup, app_handle) = Actor::spawn(
        None,
        ApplicationSupervisor,
        event_store,
    ).await.unwrap();

    // Verify SessionSupervisor exists
    let session_sup: ActorRef<SessionSupervisorMsg> = ractor::call!(
        app_sup,
        |reply| ApplicationSupervisorMsg::GetSessionSupervisor { reply }
    ).await.unwrap();

    assert!(session_sup.get_id().is_local());

    // Cleanup
    app_sup.stop(None);
    let _ = app_handle.await;
}

#[tokio::test]
async fn test_supervisor_restarts_failed_child() {
    // Spawn supervisor with test child that will crash
    // Kill child actor
    // Verify supervisor restarts child with same name
    // Verify new ActorId is different (fresh spawn)
}
```

#### Success Criteria
- [ ] ApplicationSupervisor spawns successfully
- [ ] SessionSupervisor is spawned as child
- [ ] Feature flag controls which code path is used
- [ ] All existing tests pass with feature flag disabled
- [ ] New supervision tests pass with feature flag enabled

---

### Phase 2: Desktop Domain Migration - Week 2
**Goal:** Migrate DesktopActor to supervision tree
**Risk:** Medium

#### Tasks

1. **Implement DesktopSupervisor** (`src/supervisors/desktop_supervisor.rs`)
   - `one_for_one` restart strategy
   - Spawns named DesktopActor instances
   - Handles registry-based discovery

2. **Update DesktopActor for supervision**
   - Ensure proper `post_stop` cleanup
   - Add graceful shutdown message variant
   - Persist window state to EventStore before stop

3. **Integrate with SessionSupervisor**
   - SessionSupervisor spawns DesktopSupervisor as child
   - Route desktop requests through supervision tree

4. **Create compatibility layer**
   - Keep `ActorManager::get_or_create_desktop` API
   - Internally route to DesktopSupervisor when feature flag enabled

#### Code Example: DesktopSupervisor

```rust
// src/supervisors/desktop_supervisor.rs

#[derive(Debug, Default)]
pub struct DesktopSupervisor;

pub struct DesktopSupervisorState {
    child_args: HashMap<ractor::ActorId, DesktopArguments>,
    restart_counts: HashMap<ractor::ActorId, (u32, Instant)>,
    event_store: ActorRef<EventStoreMsg>,
}

#[derive(Debug)]
pub enum DesktopSupervisorMsg {
    GetOrCreateDesktop {
        desktop_id: String,
        user_id: String,
        reply: ractor::RpcReplyPort<ActorRef<DesktopActorMsg>>,
    },
    CloseDesktop {
        desktop_id: String,
        reply: ractor::RpcReplyPort<Result<(), String>>,
    },
}

const MAX_RESTARTS: u32 = 3;
const RESTART_WINDOW: Duration = Duration::from_secs(60);

#[ractor::async_trait]
impl Actor for DesktopSupervisor {
    type Msg = DesktopSupervisorMsg;
    type State = DesktopSupervisorState;
    type Arguments = ActorRef<EventStoreMsg>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        event_store: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            supervisor_id = %myself.get_id(),
            "DesktopSupervisor starting"
        );

        Ok(DesktopSupervisorState {
            child_args: HashMap::new(),
            restart_counts: HashMap::new(),
            event_store,
        })
    }

    async fn handle_supervision_event(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match event {
            SupervisionEvent::ActorFailed(actor_cell, error) => {
                tracing::error!(
                    supervisor_id = %myself.get_id(),
                    failed_actor = %actor_cell.get_id(),
                    error = %error,
                    "DesktopActor failed"
                );

                // Check restart intensity
                let actor_id = actor_cell.get_id();
                let (count, window_start) = state.restart_counts
                    .get(&actor_id)
                    .copied()
                    .unwrap_or((0, Instant::now()));

                let should_restart = if Instant::now().duration_since(window_start) > RESTART_WINDOW {
                    // Reset window
                    true
                } else if count < MAX_RESTARTS {
                    true
                } else {
                    false
                };

                if should_restart {
                    if let Some(args) = state.child_args.get(&actor_id) {
                        let actor_name = format!("desktop:{}", args.desktop_id);
                        
                        tracing::info!(
                            desktop_id = %args.desktop_id,
                            restart_count = count + 1,
                            "Restarting DesktopActor"
                        );

                        let (new_ref, _) = Actor::spawn(
                            Some(actor_name),
                            DesktopActor,
                            args.clone(),
                        ).await?;

                        // Update restart tracking
                        let new_count = if Instant::now().duration_since(window_start) > RESTART_WINDOW {
                            1
                        } else {
                            count + 1
                        };
                        state.restart_counts.insert(new_ref.get_id(), (new_count, Instant::now()));
                        state.child_args.insert(new_ref.get_id(), args.clone());
                    }
                } else {
                    tracing::error!(
                        actor_id = %actor_id,
                        "Restart intensity exceeded, giving up"
                    );
                    state.child_args.remove(&actor_id);
                    state.restart_counts.remove(&actor_id);
                }
            }
            SupervisionEvent::ActorTerminated(actor_cell, _, exit_reason) => {
                let actor_id = actor_cell.get_id();
                
                // Check if graceful shutdown
                let is_graceful = exit_reason
                    .as_ref()
                    .map(|r| r.contains("graceful"))
                    .unwrap_or(false);

                if is_graceful {
                    tracing::info!(
                        actor_id = %actor_id,
                        "DesktopActor terminated gracefully"
                    );
                    state.child_args.remove(&actor_id);
                    state.restart_counts.remove(&actor_id);
                } else {
                    // Treat as failure
                    tracing::warn!(
                        actor_id = %actor_id,
                        exit_reason = ?exit_reason,
                        "DesktopActor terminated abnormally"
                    );
                    // Trigger restart logic (similar to ActorFailed)
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            DesktopSupervisorMsg::GetOrCreateDesktop { desktop_id, user_id, reply } => {
                let actor_name = format!("desktop:{}", desktop_id);
                
                // Check registry first
                if let Some(cell) = ractor::registry::where_is(actor_name.clone()) {
                    tracing::debug!(
                        desktop_id = %desktop_id,
                        "DesktopActor found in registry"
                    );
                    let _ = reply.send(cell.into());
                } else {
                    // Spawn new DesktopActor
                    let args = DesktopArguments {
                        desktop_id: desktop_id.clone(),
                        user_id: user_id.clone(),
                        event_store: state.event_store.clone(),
                    };

                    tracing::info!(
                        desktop_id = %desktop_id,
                        "Creating new DesktopActor"
                    );

                    let (actor_ref, _) = Actor::spawn(
                        Some(actor_name),
                        DesktopActor,
                        args.clone(),
                    ).await?;

                    // Track for restart
                    state.child_args.insert(actor_ref.get_id(), args);
                    state.restart_counts.insert(actor_ref.get_id(), (0, Instant::now()));

                    let _ = reply.send(actor_ref);
                }
            }
            DesktopSupervisorMsg::CloseDesktop { desktop_id, reply } => {
                let actor_name = format!("desktop:{}", desktop_id);
                
                if let Some(cell) = ractor::registry::where_is(actor_name) {
                    let actor_ref: ActorRef<DesktopActorMsg> = cell.into();
                    
                    // Send graceful shutdown message
                    // DesktopActor should handle this and exit cleanly
                    actor_ref.stop(Some("graceful_shutdown".to_string()));
                    
                    let _ = reply.send(Ok(()));
                } else {
                    let _ = reply.send(Err("Desktop not found".to_string()));
                }
            }
        }
        Ok(())
    }
}
```

#### Testing Strategy

```rust
// tests/desktop_supervision_test.rs

#[tokio::test]
async fn test_desktop_actor_restart_preserves_identity() {
    let (event_store, _) = spawn_event_store().await;
    let (desktop_sup, _) = spawn_desktop_supervisor(event_store).await;

    // Create desktop
    let desktop = get_or_create_desktop(&desktop_sup, "desk-1", "user-1").await;
    let original_id = desktop.get_id();

    // Crash the desktop actor (simulate panic)
    // In real test, we'd inject a "crash me" message
    desktop.stop(Some("simulated_crash".to_string()));
    
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify actor is restarted
    let restarted = get_or_create_desktop(&desktop_sup, "desk-1", "user-1").await;
    
    // Should be different ActorId (new instance)
    assert_ne!(restarted.get_id(), original_id);
    
    // But registry should return the new one
    let from_registry = ractor::registry::where_is("desktop:desk-1".to_string());
    assert!(from_registry.is_some());
    assert_eq!(from_registry.unwrap().get_id(), restarted.get_id());
}

#[tokio::test]
async fn test_desktop_restart_intensity_limit() {
    // Spawn desktop
    // Crash it 4 times in rapid succession
    // Verify supervisor stops trying to restart after 3rd failure
    // Verify supervisor logs error about intensity exceeded
}
```

#### Success Criteria
- [ ] DesktopSupervisor implements `one_for_one` restart
- [ ] DesktopActors are automatically restarted on crash
- [ ] Registry-based discovery works correctly
- [ ] Restart intensity limits are enforced (3 in 60s)
- [ ] WebSocket connections can reconnect to restarted actors
- [ ] Window state is preserved via EventStore across restarts

---

### Phase 3: Terminal Factory Pattern - Weeks 3-4
**Goal:** Replace manual terminal management with ractor::factory worker pools
**Risk:** High (involves PTY lifecycle, complex)

#### Tasks

1. **Implement TerminalWorker** (`src/actors/terminal_worker.rs`)
   - Implement `ractor::factory::Worker` trait
   - Manage single PTY session lifecycle
   - Handle key-based routing (terminal_id → worker)

2. **Create TerminalFactory** (`src/supervisors/terminal_factory.rs`)
   - Use `ractor::factory::Factory` for worker pool
   - Configure `KeyPersistentRouting` (same terminal_id → same worker)
   - Set bounded queue (max 1000 messages) for backpressure

3. **Update TerminalSupervisor**
   - Manage TerminalFactory lifecycle
   - Handle factory restarts (which recreate all workers)
   - Map external terminal_id to factory worker

4. **Migrate TerminalActor functionality**
   - Port PTY management to TerminalWorker
   - Ensure output buffering works with factory routing
   - Handle worker restart (reattach to existing PTY or spawn new)

5. **Update API layer**
   - Route terminal requests through factory
   - Handle factory-level backpressure
   - Graceful degradation when factory is at capacity

#### Code Example: TerminalWorker

```rust
// src/actors/terminal_worker.rs

use ractor::factory::{Worker, WorkerId, FactoryMessage};
use ractor::{ActorProcessingErr, ActorRef};

pub struct TerminalWorker;

pub struct TerminalWorkerState {
    terminal_id: String,
    pty_master: Option<Box<dyn portable_pty::MasterPty + Send>>,
    child_killer: Option<Box<dyn ChildKiller + Send + Sync>>,
    output_tx: Option<broadcast::Sender<String>>,
    output_buffer: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TerminalJob {
    pub terminal_id: String,
    pub message: TerminalWorkerMessage,
}

#[derive(Debug)]
pub enum TerminalWorkerMessage {
    Start {
        shell: String,
        working_dir: String,
    },
    SendInput(String),
    Resize { rows: u16, cols: u16 },
    GetOutput,
    Stop,
}

#[ractor::async_trait]
impl Worker for TerminalWorker {
    type Key = String;  // terminal_id
    type Message = TerminalWorkerMessage;
    type State = TerminalWorkerState;
    type Arguments = ();

    async fn pre_start(
        &self,
        worker_id: WorkerId,
        factory: &ActorRef<FactoryMessage<String, TerminalWorkerMessage>>,
        _args: (),
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!(
            worker_id = worker_id.get_worker_index(),
            "TerminalWorker starting"
        );

        Ok(TerminalWorkerState {
            terminal_id: String::new(),
            pty_master: None,
            child_killer: None,
            output_tx: None,
            output_buffer: Vec::with_capacity(1000),
        })
    }

    async fn handle(
        &self,
        worker_id: WorkerId,
        factory: &ActorRef<FactoryMessage<String, TerminalWorkerMessage>>,
        job: ractor::factory::Job<String, TerminalWorkerMessage>,
        state: &mut Self::State,
    ) -> Result<String, ActorProcessingErr> {
        match job.msg {
            TerminalWorkerMessage::Start { shell, working_dir } => {
                state.terminal_id = job.key.clone();
                
                // Spawn PTY (similar to current TerminalActor::Start)
                let (pty_master, child_killer, output_tx) = spawn_pty(
                    &shell,
                    &working_dir,
                    24,
                    80,
                ).await?;

                state.pty_master = Some(pty_master);
                state.child_killer = Some(child_killer);
                state.output_tx = Some(output_tx);

                tracing::info!(
                    terminal_id = %job.key,
                    worker_id = worker_id.get_worker_index(),
                    "Terminal PTY started"
                );
            }
            TerminalWorkerMessage::SendInput(input) => {
                if let Some(ref mut pty_master) = state.pty_master {
                    // Write input to PTY
                    use std::io::Write;
                    if let Err(e) = pty_master.write_all(input.as_bytes()) {
                        tracing::error!("Failed to write to PTY: {}", e);
                    }
                }
            }
            TerminalWorkerMessage::Resize { rows, cols } => {
                if let Some(ref mut pty_master) = state.pty_master {
                    let _ = pty_master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
            }
            TerminalWorkerMessage::GetOutput => {
                // Return buffered output
                // In real implementation, this would be an RPC-style reply
            }
            TerminalWorkerMessage::Stop => {
                if let Some(mut killer) = state.child_killer.take() {
                    let _ = killer.kill();
                }
                state.pty_master = None;
                state.output_tx = None;
                
                tracing::info!(
                    terminal_id = %state.terminal_id,
                    "Terminal stopped"
                );
            }
        }

        Ok(job.key)
    }
}

// Worker builder for factory
pub struct TerminalWorkerBuilder;

impl ractor::factory::WorkerBuilder<TerminalWorker, ()> for TerminalWorkerBuilder {
    fn build(&mut self, _wid: usize) -> (TerminalWorker, ()) {
        (TerminalWorker, ())
    }
}
```

#### Code Example: TerminalFactory Setup

```rust
// src/supervisors/terminal_factory.rs

use ractor::factory::{self, Factory, FactoryArguments, routing::KeyPersistentRouting, queues::DefaultQueue};
use ractor::Actor;

pub async fn create_terminal_factory(
    user_id: String,
) -> Result<ActorRef<factory::FactoryMessage<String, TerminalWorkerMessage>>, ractor::ActorProcessingErr> {
    let factory_name = format!("terminal_factory:{}", user_id);

    // Check if already exists
    if let Some(cell) = ractor::registry::where_is(factory_name.clone()) {
        return Ok(cell.into());
    }

    let factory_args = FactoryArguments::builder()
        .worker_builder(Box::new(TerminalWorkerBuilder))
        .queue(DefaultQueue::default())
        .router(KeyPersistentRouting::new())
        .num_initial_workers(0)  // Spawn on-demand
        .build();

    let (factory_ref, _handle) = Actor::spawn(
        Some(factory_name),
        factory,
        factory_args,
    ).await?;

    Ok(factory_ref)
}
```

#### Testing Strategy

```rust
// tests/terminal_factory_test.rs

#[tokio::test]
async fn test_terminal_factory_routes_by_key() {
    let factory = create_terminal_factory("user-1".to_string()).await.unwrap();

    // Start terminal with specific ID
    factory.cast(factory::FactoryMessage::Dispatch(factory::Job {
        key: "term-123".to_string(),
        msg: TerminalWorkerMessage::Start {
            shell: "/bin/bash".to_string(),
            working_dir: "/".to_string(),
        },
        options: factory::JobOptions::default(),
        accepted: None,
    })).await.unwrap();

    // Send input to same terminal ID
    factory.cast(factory::FactoryMessage::Dispatch(factory::Job {
        key: "term-123".to_string(),
        msg: TerminalWorkerMessage::SendInput("echo hello\n".to_string()),
        options: factory::JobOptions::default(),
        accepted: None,
    })).await.unwrap();

    // Verify both messages go to same worker
    // (would need to instrument worker to verify)
}

#[tokio::test]
async fn test_terminal_worker_restart_reattaches_pty() {
    // Start terminal
    // Verify PTY process exists
    // Kill worker (simulate crash)
    // Verify factory restarts worker
    // Verify new worker can still interact with same PTY (if possible)
    // Or verify old PTY is cleaned up and new PTY spawned
}

#[tokio::test]
async fn test_terminal_factory_backpressure() {
    // Create factory with small queue (size=5)
    // Rapidly dispatch 10 jobs
    // Verify some are rejected or delayed
    // Verify no memory exhaustion
}
```

#### Success Criteria
- [ ] TerminalFactory uses `ractor::factory` with KeyPersistentRouting
- [ ] Each terminal_id routes to consistent worker
- [ ] Factory restarts crashed workers automatically
- [ ] Bounded queue prevents memory exhaustion
- [ ] PTY processes are properly cleaned up on worker restart
- [ ] WebSocket terminal connections survive worker restarts

---

### Phase 4: Chat Domain Migration - Week 5
**Goal:** Migrate ChatActor and ChatAgent to supervision tree
**Risk:** Medium

#### Tasks

1. **Implement ChatSupervisor** (`src/supervisors/chat_supervisor.rs`)
   - `one_for_one` restart strategy
   - Spawn both ChatActor and ChatAgent
   - Handle agent state persistence on restart

2. **Update ChatActor for supervision**
   - Ensure event history is loaded from EventStore on restart
   - Persist pending messages before stop

3. **Update ChatAgent for supervision**
   - Persist LLM context to EventStore periodically
   - Restore conversation history from events on restart

4. **Integrate with SessionSupervisor**
   - Route chat requests through supervision tree

5. **Update API layer**
   - Route chat WebSocket connections through supervisor

#### State Persistence Pattern

```rust
// src/actors/chat_agent.rs (additions for persistence)

impl ChatAgent {
    async fn persist_state(&self, state: &ChatAgentState) -> Result<(), ChatAgentError> {
        let context = serde_json::json!({
            "messages": state.messages,
            "current_model": state.current_model,
            "message_count": state.messages.len(),
        });

        self.log_event(
            state,
            "chat_agent.state_snapshot",
            context,
            state.args.user_id.clone(),
        ).await
    }

    async fn restore_state(&self, args: &ChatAgentArguments) -> Result<Vec<BamlMessage>, ChatAgentError> {
        // Load from EventStore
        let events = get_events_for_actor(&args.event_store, &args.actor_id, 0).await?;
        
        // Find most recent state snapshot
        let snapshot = events.iter()
            .filter(|e| e.event_type == "chat_agent.state_snapshot")
            .last();

        if let Some(snapshot) = snapshot {
            // Restore from snapshot
            let state: PersistedState = serde_json::from_value(snapshot.payload.clone())?;
            Ok(state.messages)
        } else {
            // Reconstruct from message events
            Ok(Self::history_from_events(events))
        }
    }
}

#[ractor::async_trait]
impl Actor for ChatAgent {
    // ...

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Restore persisted state
        let messages = self.restore_state(&args).await.unwrap_or_default();

        Ok(ChatAgentState {
            args,
            messages,
            tool_registry: Arc::new(ToolRegistry::new()),
            current_model: "ClaudeBedrock".to_string(),
        })
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Persist final state
        let _ = self.persist_state(state).await;
        Ok(())
    }
}
```

#### Testing Strategy

```rust
// tests/chat_supervision_test.rs

#[tokio::test]
async fn test_chat_agent_restart_preserves_conversation() {
    let (event_store, _) = spawn_event_store().await;
    let (chat_sup, _) = spawn_chat_supervisor(event_store.clone()).await;

    // Create chat and send messages
    let chat = get_or_create_chat(&chat_sup, "chat-1", "user-1").await;
    send_user_message(&chat, "Hello").await.unwrap();
    
    // Get agent
    let agent = get_or_create_agent(&chat_sup, "agent-1", "user-1").await;
    let _ = process_message(&agent, "How are you?").await;

    // Crash agent
    agent.stop(Some("crash".to_string()));
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Restart agent
    let restarted = get_or_create_agent(&chat_sup, "agent-1", "user-1").await;
    
    // Verify history is restored
    let history = get_conversation_history(&restarted).await.unwrap();
    assert!(history.iter().any(|m| m.content == "Hello"));
    assert!(history.iter().any(|m| m.content == "How are you?"));
}
```

#### Success Criteria
- [ ] ChatSupervisor manages both ChatActor and ChatAgent
- [ ] Conversation history persists across restarts
- [ ] LLM context is restored after agent restart
- [ ] Pending messages are not lost on restart
- [ ] WebSocket chat connections can reconnect

---

### Phase 5: Cleanup and Deprecation - Week 6
**Goal:** Remove legacy ActorManager and complete migration
**Risk:** Low

#### Tasks

1. **Update all API handlers** (`src/api/*.rs`)
   - Replace `ActorManager` usage with supervisor references
   - Update route handlers to use new supervision-based APIs

2. **Remove ActorManager** (`src/actor_manager.rs`)
   - Delete the entire module
   - Remove DashMap and Mutex dependencies if no longer used elsewhere

3. **Update AppState** (`src/main.rs`)
   - Replace `actor_manager: ActorManager` with supervisor references
   - Update all middleware and handlers

4. **Update documentation**
   - Update AGENTS.md with new architecture
   - Update API documentation
   - Add supervision troubleshooting guide

5. **Remove feature flag**
   - Remove `supervision_refactor` feature
   - Make supervision tree the only code path

6. **Final testing**
   - Full test suite: `just test`
   - Integration tests with WebSocket clients
   - Chaos testing (random actor kills)

#### Cleanup Checklist

- [ ] No `ActorManager` references in codebase
- [ ] No DashMap usage for actor registry
- [ ] No Mutex for actor coordination
- [ ] All actors spawned through supervisors
- [ ] All tests pass
- [ ] Documentation updated
- [ ] Feature flag removed

---

## 4. Testing Strategy

### 4.1 Unit Tests

Create test modules for each supervisor:

```rust
// tests/supervisors/mod.rs

mod application_supervisor_test;
mod session_supervisor_test;
mod desktop_supervisor_test;
mod chat_supervisor_test;
mod terminal_supervisor_test;
```

Each test module should cover:
- Supervisor startup and initialization
- Child spawning and linking
- Restart on failure
- Restart intensity limits
- Graceful shutdown
- Registry cleanup

### 4.2 Integration Tests

Create comprehensive integration tests:

```rust
// tests/supervision_integration_test.rs

#[tokio::test]
async fn test_full_supervision_tree_startup() {
    // Start entire supervision tree
    // Verify all supervisors are spawned
    // Verify EventStore is accessible
    // Create actors in each domain
    // Verify registry entries exist
}

#[tokio::test]
async fn test_cascading_restart_on_session_supervisor_failure() {
    // Start full tree
    // Create actors in all domains
    // Kill SessionSupervisor
    // Verify ApplicationSupervisor restarts it
    // Verify all domain supervisors are restarted
    // Verify actors are recreated
}

#[tokio::test]
async fn test_websocket_reconnection_after_actor_restart() {
    // Connect WebSocket to terminal
    // Send commands
    // Crash TerminalActor
    // Verify supervisor restarts it
    // Verify WebSocket can reconnect and resume
}
```

### 4.3 Chaos Testing

Implement chaos testing to verify fault tolerance:

```rust
// tests/chaos_test.rs

#[tokio::test]
async fn test_random_actor_kills() {
    let app_sup = spawn_full_application().await;
    
    // Create various actors
    let desktop = get_or_create_desktop(&app_sup, "d1", "u1").await;
    let chat = get_or_create_chat(&app_sup, "c1", "u1").await;
    let terminal = get_or_create_terminal(&app_sup, "t1", "u1").await;

    // Randomly kill actors
    let mut rng = rand::thread_rng();
    for _ in 0..10 {
        let target = rng.gen_range(0..3);
        match target {
            0 => desktop.stop(Some("chaos".to_string())),
            1 => chat.stop(Some("chaos".to_string())),
            2 => terminal.stop(Some("chaos".to_string())),
            _ => unreachable!(),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Verify system is still functional
    // All actors should be restarted
    let desktop2 = get_or_create_desktop(&app_sup, "d1", "u1").await;
    let chat2 = get_or_create_chat(&app_sup, "c1", "u1").await;
    let terminal2 = get_or_create_terminal(&app_sup, "t1", "u1").await;

    // Verify they work
    assert!(desktop2.get_id().is_local());
    assert!(chat2.get_id().is_local());
    assert!(terminal2.get_id().is_local());
}
```

### 4.4 Load Testing

Test behavior under load:

```bash
# Run load test with many concurrent actors
cargo test --test load_test -- --nocapture
```

```rust
// tests/load_test.rs

#[tokio::test]
async fn test_concurrent_actor_creation() {
    let app_sup = spawn_full_application().await;
    
    // Spawn 100 desktop actors concurrently
    let mut handles = vec![];
    for i in 0..100 {
        let sup = app_sup.clone();
        let handle = tokio::spawn(async move {
            get_or_create_desktop(&sup, format!("desktop-{}", i), "user-1").await
        });
        handles.push(handle);
    }

    // All should succeed
    for handle in handles {
        let actor = handle.await.unwrap();
        assert!(actor.get_id().is_local());
    }

    // Verify all in registry
    for i in 0..100 {
        let name = format!("desktop:desktop-{}", i);
        assert!(ractor::registry::where_is(name).is_some());
    }
}
```

---

## 5. Risk Assessment and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| **Actor state loss on restart** | Medium | High | Persist critical state to EventStore in `post_stop`, restore in `pre_start` |
| **WebSocket connections drop** | High | Medium | Implement reconnection logic in frontend; use registry for fresh ActorRef |
| **Registry name collisions** | Low | High | Use consistent naming scheme; include user_id and session_id |
| **Memory leaks from crashed actors** | Low | High | Use supervision events for cleanup; registry auto-cleans on drop |
| **Deadlocks in supervision tree** | Low | High | Use `one_for_one` strategy; avoid synchronous blocking in supervisors |
| **Performance regression** | Medium | Medium | Benchmark before/after; factory pattern provides backpressure |
| **Migration breaks existing features** | Medium | High | Feature flag approach; extensive integration tests |

---

## 6. Timeline and Milestones

```
Week 1: Phase 1 - Foundation
├── Day 1-2: Create supervisor module structure
├── Day 3-4: Implement ApplicationSupervisor
├── Day 5: Implement SessionSupervisor
└── Milestone: Supervision tree starts successfully

Week 2: Phase 2 - Desktop Migration
├── Day 1-2: Implement DesktopSupervisor
├── Day 3: Update DesktopActor for persistence
├── Day 4: Integration with SessionSupervisor
├── Day 5: Testing and bug fixes
└── Milestone: Desktop actors restart automatically

Week 3: Phase 3 - Terminal Factory (Part 1)
├── Day 1-3: Implement TerminalWorker
├── Day 4-5: Create TerminalFactory
└── Milestone: Terminal factory spawns workers

Week 4: Phase 3 - Terminal Factory (Part 2)
├── Day 1-2: Update TerminalSupervisor
├── Day 3-4: Update API layer
├── Day 5: Testing and backpressure validation
└── Milestone: Terminal sessions survive worker restarts

Week 5: Phase 4 - Chat Migration
├── Day 1-2: Implement ChatSupervisor
├── Day 3: Update ChatActor persistence
├── Day 4: Update ChatAgent persistence
├── Day 5: Integration testing
└── Milestone: Chat conversations persist across restarts

Week 6: Phase 5 - Cleanup
├── Day 1-2: Remove ActorManager
├── Day 3: Update documentation
├── Day 4: Final testing
├── Day 5: Buffer for issues
└── Milestone: Migration complete, all tests pass
```

---

## 7. Success Criteria

### 7.1 Functional Requirements

- [ ] All actors are supervised (none spawned directly without supervision)
- [ ] Crashed actors are automatically restarted within 1 second
- [ ] Actor identity is preserved across restarts (same name in registry)
- [ ] State persistence works for ChatAgent and DesktopActor
- [ ] WebSocket connections can reconnect to restarted actors
- [ ] Terminal sessions survive worker restarts (PTY reattached or cleanly recreated)
- [ ] No memory leaks after repeated crash/restart cycles
- [ ] System remains stable under 100 concurrent actors

### 7.2 Non-Functional Requirements

- [ ] Latency increase < 10% for actor operations
- [ ] No deadlocks under load testing
- [ ] CPU usage remains within 20% of baseline
- [ ] All existing tests pass
- [ ] Code coverage > 80% for new supervisor modules
- [ ] Documentation complete and accurate

### 7.3 Monitoring and Observability

Add metrics for:
- Actor restart rate per supervisor
- Supervision event counts (started, terminated, failed)
- Registry entry count
- Factory queue depth and backpressure events
- Time to restart after failure

---

## 8. References

### 8.1 Documentation

- [Ractor Documentation](https://docs.rs/ractor/latest/ractor/)
- [Ractor Supervision Guide](https://slawlor.github.io/ractor/faq/)
- [Erlang/OTP Supervision Principles](https://www.erlang.org/doc/design_principles/sup_princ)
- [ractor-supervisor Crate](https://docs.rs/ractor-supervisor/latest/ractor_supervisor/)

### 8.2 Related Code

- Current: `sandbox/src/actor_manager.rs` (anti-patterns)
- Current: `sandbox/src/actors/*.rs` (actor implementations)
- Target: `sandbox/src/supervisors/*.rs` (new supervision tree)
- Reference: `docs/architecture/ractor-supervision-best-practices.md`

### 8.3 Testing Resources

- `tests/desktop_api_test.rs` (existing integration tests)
- `tests/terminal_test.rs` (terminal-specific tests)
- `tests/chat_test.rs` (chat-specific tests)

---

## 9. Appendices

### Appendix A: Glossary

| Term | Definition |
|------|------------|
| **Supervision Tree** | Hierarchical structure of supervisors and workers |
| **OneForOne** | Restart strategy: only failed child is restarted |
| **RestForOne** | Restart strategy: failed child and all later children restarted |
| **Factory Pattern** | Worker pool pattern for managing multiple actors |
| **Registry** | ractor's built-in actor naming and discovery system |
| **SupervisionEvent** | Lifecycle events sent to supervisors (started, failed, terminated) |
| **Restart Intensity** | Maximum restarts allowed within a time window |

### Appendix B: Migration Checklist

**Before Starting:**
- [ ] Review this document with team
- [ ] Set up feature flag infrastructure
- [ ] Create test scaffolding
- [ ] Backup current codebase

**Per Phase:**
- [ ] Implement phase requirements
- [ ] Write unit tests
- [ ] Write integration tests
- [ ] Run existing tests (must pass)
- [ ] Performance benchmark
- [ ] Code review
- [ ] Update documentation
- [ ] Merge to main

**After Completion:**
- [ ] Remove feature flag
- [ ] Delete deprecated code
- [ ] Final integration testing
- [ ] Production deployment plan
- [ ] Rollback procedure documented

---

*Document Version: 1.0*
*Created: 2026-02-06*
*Last Updated: 2026-02-06*
