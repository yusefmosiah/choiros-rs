# Ractor Supervision Best Practices for ChoirOS

## Executive Summary

This guide provides actionable, implementation-ready patterns for using `ractor` correctly in ChoirOS, with emphasis on replacing mutex-heavy coordination patterns with actor-native supervision and messaging. The current implementation in `actor_manager.rs` uses external concurrency primitives (DashMap, Mutex) for actor discovery and lifecycle management, which defeats the purpose of using an actor framework.

**Key Insight:** In ractor (and Erlang/OTP), actors are supervised by linking them together in a tree. Supervisors receive lifecycle events from their children via `SupervisionEvent` messages and can restart failed actors automatically. External locking mechanisms create unnecessary bottlenecks and prevent proper fault isolation.

---

## What to Stop Doing / What to Start Doing

| ❌ Stop Doing (Current Anti-Patterns) | ✅ Start Doing (Actor-Native Patterns) | Rationale |
|--------------------------------------------|------------------------------------------|-----------|
| **Using DashMap for actor registry** (lines 31-34 in `actor_manager.rs`) | **Use ractor's built-in registry** (`ractor::registry::where_is`) | DashMap requires explicit manual cleanup; ractor's registry auto-unregisters on actor drop, preventing name collisions |
| **Using Mutex for serialization** (line 35 `terminal_create_lock`) | **Use supervision tree and `SupervisionEvent` ordering** | Mutexes block all threads; supervision events are prioritized and guarantee ordering via actor mailbox |
| **Direct Actor::spawn calls in get_or_create methods** | **Spawn children from supervisor via `spawn_linked` or manage via supervision tree** | Unsupervised spawns can't be automatically restarted; supervisor owns child lifecycle |
| **Manual remove_terminal cleanup** (lines 207-211) | **Let supervisor handle child termination via `SupervisionEvent::ActorTerminated` | Manual cleanup races with supervisor events; supervisor already knows when child dies |
| **Stale ActorRef in DashMap after crash** | **Supervisor restarts failed actors with fresh `ActorRef`** | Stale refs cause message loss; supervisor provides clean lifecycle management |
| **No backpressure on terminal creation** | **Use factory pattern with bounded queues for high-velocity actors** | Unbounded spawns can overwhelm system; factory provides controlled worker pools |
| **Cross-actor coordination via shared state** | **Coordinate via message passing only** | Shared state breaks actor isolation and introduces race conditions |

---

## Proposed Supervision Tree for ChoirOS Backend

### Current Architecture Issues

```
┌─────────────────────────────────────────────────────────────┐
│  ActorManager (DashMap + Mutex)                     │
│  ┌────────────┬─────────────┬─────────────┐│
│  │chat_actors  │chat_agents  │desktop_actors││
│  │DashMap     │DashMap     │DashMap     ││
│  └────────────┴─────────────┴─────────────┘│
└─────────────────────────────────────────────────────────────┘
     └─> Direct Actor::spawn (unsupervised)
```

**Problems:**
- No fault containment: crashed actors leave stale `ActorRef` in DashMap
- No restart policy: manual intervention required for failures
- Registry inconsistency: manual `remove_terminal` can race with supervisor
- Mutex contention: single lock blocks all actor lookups

### Target Architecture

```
┌───────────────────────────────────────────────────────────────┐
│  ApplicationSupervisor                                          │
│  ┌────────────────────────────────────────────────────┐   │
│  │ SessionSupervisor (one_for_one strategy)       │   │
│  │ ┌──────────┬──────────┬────────────┐│   │
│  │ │DesktopSup│ChatSup   │TerminalSuperv││   │
│  │ │ervisor    │ervisor    │isor        ││   │
│  │ └──────────┴──────────┴────────────┘│   │
│  │   ▲ one_for_one for each domain   │   │
│  └────────────────────────────────────────────────────┘   │
│         ▲ rest_for_one strategy (cascading)        │
└───────────────────────────────────────────────────────────────┘
```

**Benefits:**
- **Fault isolation**: Each domain supervisor restarts only failed actors
- **Automatic recovery**: `SupervisionEvent::ActorFailed` triggers restarts
- **No locks**: All coordination via message passing
- **Registry via naming**: Use `Actor::spawn(Some("name"))` for discovery

### Component Responsibilities

| Component | Responsibility | Supervision Strategy |
|------------|--------------|----------------------|
| **ApplicationSupervisor** | Root supervisor; spawns domain supervisors; restarts crashed domain supervisors | `rest_for_one` (cascading restart) |
| **SessionSupervisor** | Manages session-scoped actors (desktop, chat, terminal pools); monitors domain supervisors | `one_for_one` (isolated restarts) |
| **DesktopSupervisor** | Spawns and supervises per-user `DesktopActor` instances | `one_for_one` with restart intensity=3, period=60 |
| **ChatSupervisor** | Spawns and supervises per-chat `ChatActor` instances | `one_for_one` with restart intensity=5, period=30 |
| **TerminalSupervisor** | Uses `ractor::factory` for terminal workers; manages PTY lifecycle | `simple_one_for_one` with dynamic child spawning |

---

## Refactor Blueprint: actor_manager.rs

### Before (Current Anti-Pattern)

```rust
// sandbox/src/actor_manager.rs

pub struct ActorManager {
    chat_actors: Arc<DashMap<String, ActorRef<ChatActorMsg>>>,
    chat_agents: Arc<DashMap<String, ActorRef<ChatAgentMsg>>>,
    desktop_actors: Arc<DashMap<String, ActorRef<DesktopActorMsg>>>,
    terminal_actors: Arc<DashMap<String, ActorRef<TerminalMsg>>>,
    terminal_create_lock: Arc<Mutex<()>>,  // ❌ Mutex coordination
    event_store: ActorRef<EventStoreMsg>,
}

impl ActorManager {
    pub async fn get_or_create_terminal(
        &self,
        terminal_id: &str,
        args: TerminalArguments,
    ) -> Result<ActorRef<TerminalMsg>, ractor::ActorProcessingErr> {
        // ❌ Double-checked locking pattern
        if let Some(entry) = self.terminal_actors.get(terminal_id) {
            return Ok(entry.clone());
        }

        // ❌ Mutex block serialized creation
        let _create_guard = self.terminal_create_lock.lock().await;
        if let Some(entry) = self.terminal_actors.get(terminal_id) {
            return Ok(entry.clone());
        }

        // ❌ Unsupervised spawn - no supervision tree
        let (terminal_ref, _handle) = Actor::spawn(None, TerminalActor, args).await?;

        // ❌ Manual registry insertion
        self.terminal_actors.insert(terminal_id.to_string(), terminal_ref.clone());
        Ok(terminal_ref)
    }

    // ❌ Manual cleanup - races with supervision
    pub fn remove_terminal(&self, terminal_id: &str) -> Option<ActorRef<TerminalMsg>> {
        self.terminal_actors.remove(terminal_id).map(|entry| entry.1)
    }
}
```

**Anti-Patterns Identified:**
1. **Line 31-34**: DashMap for actor discovery
2. **Line 35**: Mutex for terminal creation serialization
3. **Lines 62-86, 101-126, 140-165, 174-198**: Direct `Actor::spawn` without supervision
4. **Lines 207-211**: Manual `remove_terminal` cleanup

### After (Actor-Native Design)

```rust
// sandbox/src/actor_manager.rs

use ractor::{registry, Actor, ActorProcessingErr, ActorRef, SupervisionEvent};
use std::collections::HashMap;

// Re-export message types
pub use crate::actors::chat::{ChatActor, ChatActorArguments, ChatActorMsg};
pub use crate::actors::chat_agent::{ChatAgent, ChatAgentArguments, ChatAgentMsg};
pub use crate::actors::desktop::{DesktopActor, DesktopActorMsg, DesktopArguments};
pub use crate::actors::event_store::EventStoreMsg;
pub use crate::actors::terminal::{TerminalActor, TerminalArguments, TerminalMsg};

/// Root application supervisor - manages domain supervisors
#[derive(Debug, Default)]
pub struct ApplicationSupervisor;

impl ApplicationSupervisor {
    async fn handle_supervision_event(
        &self,
        myself: ActorRef<ApplicationSupervisorMsg>,
        event: SupervisionEvent,
        state: &mut ApplicationState,
    ) {
        match event {
            // Restart crashed domain supervisors
            SupervisionEvent::ActorFailed(actor_cell, error) => {
                tracing::warn!(
                    supervisor = %myself.get_id(),
                    failed_actor = %actor_cell.get_id(),
                    error = %error,
                    "Domain supervisor failed, restarting"
                );

                if let Some(domain_type) = state.domain_supervisors.remove(&actor_cell.get_id()) {
                    // Restart with same configuration
                    match domain_type {
                        DomainType::Desktop => {
                            // Spawn new DesktopSupervisor (restart strategy preserved)
                        }
                        DomainType::Chat => {
                            // Spawn new ChatSupervisor
                        }
                        DomainType::Terminal => {
                            // Spawn new TerminalSupervisor
                        }
                    }
                }
            }
            SupervisionEvent::ActorTerminated(..) => {
                // Clean up domain supervisor references
                if let Some(_) = state.domain_supervisors.remove(&event.actor_cell().get_id()) {
                    tracing::info!(
                        "Domain supervisor {:?} terminated, cleaning up",
                        event.actor_cell().get_id()
                    );
                }
            }
            _ => {}
        }
    }
}

/// Application supervisor state
pub struct ApplicationState {
    /// Mapping from domain supervisor ActorId to domain type
    domain_supervisors: HashMap<ractor::ActorId, DomainType>,
    event_store: ActorRef<EventStoreMsg>,
}

enum DomainType {
    Desktop,
    Chat,
    Terminal,
}

#[derive(Debug)]
pub enum ApplicationSupervisorMsg {
    /// Get or create a desktop actor for a user
    GetOrCreateDesktop {
        desktop_id: String,
        user_id: String,
        reply: ractor::RpcReplyPort<ActorRef<DesktopActorMsg>>,
    },
    /// Get or create a chat actor
    GetOrCreateChat {
        actor_id: String,
        user_id: String,
        reply: ractor::RpcReplyPort<ActorRef<ChatActorMsg>>,
    },
    /// Get or create a chat agent
    GetOrCreateChatAgent {
        agent_id: String,
        user_id: String,
        reply: ractor::RpcReplyPort<ActorRef<ChatAgentMsg>>,
    },
    /// Get or create a terminal session
    GetOrCreateTerminal {
        terminal_id: String,
        user_id: String,
        shell: String,
        working_dir: String,
        reply: ractor::RpcReplyPort<ActorRef<TerminalMsg>>,
    },
    /// Supervision event from domain supervisors
    Supervision(SupervisionEvent),
}

#[cfg_attr(feature = "async-trait", ractor::async_trait)]
impl Actor for ApplicationSupervisor {
    type Msg = ApplicationSupervisorMsg;
    type State = ApplicationState;
    type Arguments = ActorRef<EventStoreMsg>;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        event_store: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Spawn domain supervisors as children (linked)
        // Domain supervisors will be automatically supervised

        Ok(ApplicationState {
            domain_supervisors: HashMap::new(),
            event_store,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ApplicationSupervisorMsg::GetOrCreateDesktop {
                desktop_id,
                user_id,
                reply,
            } => {
                // ✅ Use named actor discovery via ractor registry
                let actor_name = format!("desktop:{}", desktop_id);
                if let Some(actor_cell) = registry::where_is(actor_name.clone()) {
                    // Actor exists, return reference
                    let _ = reply.send(actor_cell.into());
                } else {
                    // Spawn new DesktopActor with name (auto-registered)
                    let (desktop_ref, _handle) = Actor::spawn(
                        Some(actor_name.clone()),
                        DesktopActor,
                        DesktopArguments {
                            desktop_id: desktop_id.clone(),
                            user_id: user_id.clone(),
                            event_store: state.event_store.clone(),
                        },
                    )
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to spawn DesktopActor: {}", e);
                        ActorProcessingErr::from(e)
                    })?;

                    tracing::info!(
                        "Created DesktopActor with name: {}",
                        actor_name
                    );
                    let _ = reply.send(desktop_ref);
                }
            }
            ApplicationSupervisorMsg::GetOrCreateChat {
                actor_id,
                user_id,
                reply,
            } => {
                // ✅ Similar pattern for ChatActor
                let actor_name = format!("chat:{}", actor_id);
                if let Some(actor_cell) = registry::where_is(actor_name.clone()) {
                    let _ = reply.send(actor_cell.into());
                } else {
                    let (chat_ref, _handle) = Actor::spawn(
                        Some(actor_name.clone()),
                        ChatActor,
                        ChatActorArguments {
                            actor_id: actor_id.clone(),
                            user_id: user_id.clone(),
                            event_store: state.event_store.clone(),
                        },
                    )
                    .await?;
                    let _ = reply.send(chat_ref);
                }
            }
            ApplicationSupervisorMsg::GetOrCreateChatAgent {
                agent_id,
                user_id,
                reply,
            } => {
                let actor_name = format!("chat_agent:{}", agent_id);
                if let Some(actor_cell) = registry::where_is(actor_name.clone()) {
                    let _ = reply.send(actor_cell.into());
                } else {
                    let (agent_ref, _handle) = Actor::spawn(
                        Some(actor_name.clone()),
                        ChatAgent::new(),
                        ChatAgentArguments {
                            actor_id: agent_id.clone(),
                            user_id: user_id.clone(),
                            event_store: state.event_store.clone(),
                        },
                    )
                    .await?;
                    let _ = reply.send(agent_ref);
                }
            }
            ApplicationSupervisorMsg::GetOrCreateTerminal {
                terminal_id,
                user_id,
                shell,
                working_dir,
                reply,
            } => {
                // ✅ Terminal actors use factory pattern for worker pools
                let factory_name = format!("terminal_factory:{}", user_id);
                let terminal_name = format!("terminal:{}", terminal_id);

                // Check if factory exists
                if let Some(factory_cell) = registry::where_is(factory_name.clone()) {
                    // Request factory to get/create terminal
                    // This delegates to factory which manages supervision
                } else {
                    // Create terminal factory for this user
                    // ✅ Factory provides worker pool with built-in supervision
                    use ractor::factory::{self as factory, Factory, FactoryArguments, routing::KeyPersistentRouting, queues::DefaultQueue};

                    let factory_args = FactoryArguments::builder()
                        .worker_builder(Box::new(TerminalWorkerBuilder))
                        .num_initial_workers(0)  // Start with 0, spawn on-demand
                        .queue(Default::default())
                        .router(routing::KeyPersistentRouting::new())
                        .build();

                    let (factory_ref, _factory_handle) = Actor::spawn(
                        Some(factory_name.clone()),
                        factory,
                        factory_args,
                    )
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to spawn TerminalFactory: {}", e);
                        ActorProcessingErr::from(e)
                    })?;

                    tracing::info!("Created TerminalFactory with name: {}", factory_name);
                }

                // Now request terminal from factory
                // Factory message would be: FactoryMessage::Dispatch(Job { key: terminal_id, msg: ... })
                let _ = reply.send(/* terminal actor ref from factory response */);
            }
            ApplicationSupervisorMsg::Supervision(event) => {
                // ✅ Handle supervision events from children
                self.handle_supervision_event(myself, event, state).await;
            }
        }
        Ok(())
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        tracing::info!(
            supervisor = %myself.get_id(),
            "ApplicationSupervisor stopping"
        );

        // Stop all domain supervisors
        for (domain_id, domain_type) in &state.domain_supervisors {
            // Domain supervisors will automatically stop their children
            tracing::info!("Stopping domain supervisor: {}", domain_id);
        }

        Ok(())
    }
}

/// Convenience function to get or create desktop actor
pub async fn get_or_create_desktop(
    app_supervisor: &ActorRef<ApplicationSupervisorMsg>,
    desktop_id: String,
    user_id: String,
) -> Result<ActorRef<DesktopActorMsg>, ractor::ActorProcessingErr> {
    ractor::call!(app_supervisor, |reply| {
        ApplicationSupervisorMsg::GetOrCreateDesktop {
            desktop_id,
            user_id,
            reply,
        }
    })
    .await
    .map_err(|e| ActorProcessingErr::from(e))?
}

/// Similar convenience functions for chat, chat_agent, terminal...
```

**Key Improvements:**
1. **Lines 58-68**: Actor discovery via `ractor::registry::where_is()` instead of DashMap
2. **Lines 70-83**: Named actor spawns with `Actor::spawn(Some(name), ...)` for automatic registration
3. **Lines 104-114**: Terminal factory pattern replaces manual `get_or_create_terminal`
4. **No Mutexes**: All coordination via message passing and supervision events
5. **Lines 116-125**: Supervision event handling for automatic restarts

---

## Incremental Migration Plan

### Phase 1: Foundation (Low Risk) - Week 1

**Goal**: Establish supervision tree skeleton without disrupting existing APIs

**Tasks**:
1. Create `supervisor.rs` module with `ApplicationSupervisor` and `SessionSupervisor`
2. Implement basic `SupervisionEvent` handling in `ApplicationSupervisor`
3. Add `handle_supervision_event` method for restart logic
4. Wire `ApplicationSupervisor` in `main.rs` before any other actors
5. Add feature flag `supervision_refactor` (disabled by default)

**Testing Strategy**:
- Unit tests in `tests/supervision_test.rs`:
  ```rust
  #[tokio::test]
  async fn test_supervisor_restarts_failed_child() {
      // Spawn supervisor with test child
      // Crash child actor (simulate panic)
      // Verify supervisor restarts child
      // Verify new child has same configuration
  }
  ```

**Acceptance Criteria**:
- All tests pass
- ApplicationSupervisor spawns successfully in main.rs
- Feature flag disabled: existing ActorManager still works

---

### Phase 2: Desktop Domain Migration (Medium Risk) - Week 2

**Goal**: Migrate DesktopActor to supervision tree while maintaining compatibility

**Tasks**:
1. Implement `DesktopSupervisor` with `one_for_one` restart strategy
2. Update `DesktopSupervisor` to spawn named `DesktopActor` instances
3. Add routing from `ApplicationSupervisor` → `DesktopSupervisor`
4. Back-channel: keep `ActorManager::get_or_create_desktop` but route to supervisor internally
5. Add logging for supervision events (started, failed, terminated)

**Testing Strategy**:
```rust
#[tokio::test]
async fn test_desktop_supervision_lifecycle() {
    // Spawn DesktopSupervisor
    // Request DesktopActor creation
    // Crash DesktopActor (simulate panic)
    // Verify DesktopSupervisor restarts DesktopActor
    // Verify same desktop_id, user_id preserved
    // Verify new ActorRef is returned on subsequent requests
}
```

**Acceptance Criteria**:
- DesktopActor crashes automatically restart (same identity)
- `registry::where_is("desktop:ID")` returns valid actor after restart
- Existing WebSocket connections continue working (ActorRef refreshed)

---

### Phase 3: Terminal Factory Pattern (High Risk) - Weeks 3-4

**Goal**: Replace manual terminal creation with `ractor::factory` for PTY worker pools

**Tasks**:
1. Implement `TerminalWorker` with `ractor::factory::Worker` trait
2. Create `TerminalFactory` using `Factory` from `ractor::factory`
3. Configure routing: `KeyPersistentRouting` (same terminal_id → same worker)
4. Configure queue: `DefaultQueue` with bounded capacity (e.g., 1000 messages)
5. Wire `TerminalSupervisor` → `TerminalFactory`
6. Update `api/terminal.rs` to request from factory instead of direct spawn

**Testing Strategy**:
```rust
#[tokio::test]
async fn test_terminal_factory_worker_restart() {
    // Spawn TerminalFactory with 2 workers
    // Dispatch job to terminal:123
    // Kill worker process (simulated crash)
    // Verify factory restarts worker
    // Verify in-flight jobs preserved or retried
    // Verify new worker receives subsequent jobs for same key
}

#[tokio::test]
async fn test_terminal_factory_backpressure() {
    // Spawn TerminalFactory with bounded queue (size=10)
    // Rapidly dispatch 20 jobs
    // Verify factory applies backpressure (rejects or queues)
    // Verify no unbounded memory growth
}
```

**Acceptance Criteria**:
- Factory automatically restarts crashed workers
- Same terminal_id routes to same worker (KeyPersistentRouting)
- Bounded queue prevents memory exhaustion
- PTY processes cleaned up on worker restart

---

### Phase 4: Chat Domain Migration (Medium Risk) - Week 5

**Goal**: Migrate ChatActor and ChatAgent to supervision tree

**Tasks**:
1. Implement `ChatSupervisor` with `one_for_one` restart strategy
2. Route `ApplicationSupervisor` → `ChatSupervisor` for chat requests
3. Route `ApplicationSupervisor` → `ChatSupervisor` for agent requests
4. Update `ChatActor` to persist state via EventStore on restart
5. Add supervision event logging for debugging

**Testing Strategy**:
```rust
#[tokio::test]
async fn test_chat_supervision_message_persistence() {
    // Spawn ChatSupervisor
    // Create ChatActor, send 10 messages
    // Crash ChatActor
    // Verify ChatActor restarts
    // Verify messages persisted in EventStore
    // Verify new ChatActor loads persisted state
}
```

**Acceptance Criteria**:
- ChatAgent restarts preserve LLM state (from EventStore)
- ChatActor crashes recover event history
- No message loss on restart

---

### Phase 5: Cleanup and Deprecation (Low Risk) - Week 6

**Goal**: Remove legacy `ActorManager` and transition fully to supervision tree

**Tasks**:
1. Update all API handlers (`api/*.rs`) to use supervisor instead of `ActorManager`
2. Remove `ActorManager` struct and DashMap/Mutex fields
3. Update `AppState` to hold `ApplicationSupervisor` reference
4. Deprecate `ActorManager` with `#[deprecated]` attribute
5. Update documentation and AGENTS.md with new architecture

**Testing Strategy**:
- Run full test suite: `just test`
- Manual E2E test with `agent-browser`: open terminal, crash, verify recovery
- Load test: simulate 100 concurrent terminal connections, verify stability

**Acceptance Criteria**:
- All existing tests pass
- New supervision tests pass
- No `ActorManager` references in codebase (except deprecated module)
- Documentation updated

---

## Testing Strategy by Phase

### Unit Testing

**Supervision Event Tests** (`tests/supervision_test.rs`):
```rust
use ractor::{Actor, SupervisionEvent};

#[tokio::test]
async fn test_supervisor_handles_actor_failed() {
    let (supervisor, sup_handle) = spawn_test_supervisor().await;

    // Spawn child actor
    let (child, child_handle) = spawn_test_child().await;

    // Kill child (simulates panic)
    child.kill();

    // Wait for supervision event
    let event = wait_for_supervision_event(&supervisor).await;

    match event {
        SupervisionEvent::ActorFailed(actor_cell, error) => {
            assert_eq!(actor_cell.get_id(), child.get_id());
            assert!(matches!(error, ActorProcessingErr::Panic(..)));
        }
        _ => panic!("Expected ActorFailed event"),
    }

    // Verify supervisor restarts child
    let restarted_child = registry::where_is(child_name).await;
    assert!(restarted_child.is_some());

    supervisor.stop(None);
    sup_handle.await.unwrap();
}
```

**Actor Discovery Tests** (`tests/registry_test.rs`):
```rust
#[tokio::test]
async fn test_registry_auto_cleanup_on_drop() {
    let actor_name = "test_actor".to_string();

    // Spawn named actor (auto-registered)
    let (actor, handle) = Actor::spawn(
        Some(actor_name.clone()),
        TestActor,
        (),
    ).await.unwrap();

    // Verify registered
    assert!(registry::where_is(actor_name.clone()).is_some());

    // Stop actor (triggers drop)
    actor.stop(None);
    handle.await.unwrap();

    // Verify auto-unregistered
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    assert!(registry::where_is(actor_name).is_none());
}
```

### Integration Testing

**WebSocket Reconnection Test** (`tests/terminal_supervision_integration.rs`):
```rust
#[tokio::test]
async fn test_websocket_survives_actor_restart() {
    // 1. Start WebSocket connection to terminal
    let ws = connect_websocket("terminal:123").await;

    // 2. Send commands via WebSocket
    ws.send_message("echo hello").await;
    ws.send_message("echo world").await;

    // 3. Crash TerminalActor (simulate failure)
    crash_terminal_actor("terminal:123").await;

    // 4. Wait for supervisor restart
    tokio::time::sleep(Duration::from_secs(2)).await;

    // 5. Verify WebSocket still connected and receives output
    let response = ws.receive_message().await;
    assert!(response.contains("world"));  // Output preserved
}
```

**Concurrent Stress Test** (`tests/stress_supervision.rs`):
```rust
#[tokio::test]
async fn test_rapid_spawn_crash_cycle() {
    let supervisor = spawn_application_supervisor().await;

    // Spawn 50 terminal actors concurrently
    let mut handles = vec![];
    for i in 0..50 {
        let handle = tokio::spawn(async move {
            let _ = get_or_create_terminal(&supervisor, format!("term_{}", i)).await;
            // Immediately crash
            tokio::time::sleep(Duration::from_millis(10)).await;
            crash_actor(format!("term_{}", i)).await;
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify no deadlocks or panics
    // Verify all actors eventually reach steady state
    supervisor.stop(None);
}
```

### Chaos Testing

**Kill-Switch Simulation** (`tests/chaos_kill_switch.rs`):
```rust
#[tokio::test]
async fn test_kill_switch_cascading_restart() {
    // 1. Spawn full supervision tree
    let app_sup = spawn_application_supervisor().await;

    // 2. Create multiple actors across domains
    let desktop = get_or_create_desktop(&app_sup, "desktop_1", "user_1").await;
    let terminal = get_or_create_terminal(&app_sup, "term_1", "user_1").await;
    let chat = get_or_create_chat(&app_sup, "chat_1", "user_1").await;

    // 3. Kill SessionSupervisor (cascades to all domains)
    kill_actor(session_supervisor_id).await;

    // 4. Verify ApplicationSupervisor restarts SessionSupervisor
    // 5. Verify SessionSupervisor restarts domain supervisors
    // 6. Verify domain supervisors restart individual actors

    app_sup.stop(None);
}
```

---

## Anti-Patterns in Current Architecture

### 1. DashMap for Actor Registry

**Location**: `actor_manager.rs:31-34`

**Problem**:
```rust
chat_actors: Arc<DashMap<String, ActorRef<ChatActorMsg>>>,
```

**Why it's problematic:**
- **Stale references**: If `ChatActor` crashes and restarts, DashMap still holds old `ActorRef`. Messages sent to stale ref are lost.
- **Manual cleanup**: `remove_terminal()` (line 207) requires explicit caller coordination. Forgotten cleanup = memory leak.
- **No registration guarantees**: Multiple actors can race to register same ID, creating duplicate actors.

**Correct pattern**:
```rust
// ✅ Use ractor's built-in registry
let (actor, _handle) = Actor::spawn(
    Some("chat:123".to_string()),  // Auto-registered
    ChatActor,
    args,
).await?;

// ✅ Discovery via registry
if let Some(actor_cell) = ractor::registry::where_is("chat:123".to_string()) {
    let actor_ref: ActorRef<ChatActorMsg> = actor_cell.into();
    // Use actor_ref...
}
```

**Benefits**:
- Automatic unregistration on actor drop (via `post_stop`)
- No stale references possible
- Thread-safe without explicit locks

### 2. Mutex for Serialization

**Location**: `actor_manager.rs:35, 185`

**Problem**:
```rust
terminal_create_lock: Arc<Mutex<()>>,
// ...
let _create_guard = self.terminal_create_lock.lock().await;
```

**Why it's problematic:**
- **Global bottleneck**: All terminal creations block on single mutex, even for different users/terminals.
- **Deadlock potential**: If `ActorManager` holds lock while waiting for actor spawn, and spawn fails, lock is never released.
- **Non-actor coordination**: Mutexes bypass actor isolation, breaking the model.

**Correct pattern**:
```rust
// ✅ Use supervision tree with message ordering
// No locks needed - mailbox provides serialization
// Concurrent requests handled in order of arrival
// If TerminalActor crashes, supervisor handles restart atomically
```

### 3. Unsupervised Actor Spawns

**Location**: `actor_manager.rs:71-81, 110-120, 149-159, 191`

**Problem**:
```rust
let (chat_ref, _handle) = Actor::spawn(None, ChatActor, args).await?;
// ❌ No supervision - if ChatActor crashes, it's dead forever
```

**Why it's problematic:**
- **No automatic restart**: Manual intervention required for any actor failure.
- **No fault containment**: Crashed actor leaves dangling resources (PTY, DB connections, etc.).
- **No observability**: No centralized place to log/reason about failures.

**Correct pattern**:
```rust
// ✅ Spawn as child of supervisor
let (chat_ref, _handle) = Actor::spawn(
    Some("chat:123".to_string()),
    ChatActor,
    args,
).await?;

// ✅ Supervisor receives SupervisionEvent::ActorFailed
// ✅ Supervisor can restart with same args
// ✅ Supervisor can log/monitor failure rates
```

### 4. Manual Actor Lifecycle Management

**Location**: `actor_manager.rs:207-211, 391-393 (in `api/terminal.rs`)

**Problem**:
```rust
pub fn remove_terminal(&self, terminal_id: &str) -> Option<ActorRef<TerminalMsg>> {
    self.terminal_actors.remove(terminal_id).map(|entry| entry.1)
}

// In api/terminal.rs:
terminal_actor.stop(None);
actor_manager.remove_terminal(&terminal_id);  // ❌ Manual cleanup
```

**Why it's problematic:**
- **Race with supervision**: If `terminal_actor.stop(None)` triggers `SupervisionEvent::ActorTerminated`, supervisor may already be handling restart when manual `remove_terminal` runs.
- **Inconsistent state**: Supervisor expects actor to exist; manual removal causes errors.
- **No backpressure**: No control over how many terminals can be spawned.

**Correct pattern**:
```rust
// ✅ Let supervisor handle lifecycle via SupervisionEvent
// TerminalActor.stop(None) → SupervisionEvent::ActorTerminated
// Supervisor decides: restart with same args, or clean up

// ✅ For intentional cleanup, send explicit shutdown message
terminal_actor.cast(TerminalMsg::GracefulShutdown { reason: "User disconnected".to_string() });
// TerminalActor's post_stop cleans up PTY
// Supervisor receives event and decides not to restart (based on exit reason)
```

---

## Backpressure and Mailbox Considerations

### Current Issues

**Unbounded queues**: `api/terminal.rs:92` uses `mpsc::unbounded_channel()` for WebSocket message routing. Under high load, this can cause unbounded memory growth.

### Ractor Native Solutions

**1. Factory Pattern for High-Velocity Actors**

Use `ractor::factory` for actors that receive high-frequency messages (e.g., TerminalActor):

```rust
use ractor::factory::{self as factory, Factory, FactoryArguments, routing::KeyPersistentRouting, queues::DefaultQueue};

struct TerminalWorker;
impl Worker for TerminalWorker {
    type Key = String;  // terminal_id
    type Message = TerminalMsg;
    type State = ();
    type Arguments = TerminalArguments;

    async fn pre_start(
        &self,
        worker_id: WorkerId,
        factory: &ActorRef<FactoryMessage<String, TerminalMsg>>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // Initialize PTY for this terminal
        Ok(())
    }

    async fn handle(
        &self,
        worker_id: WorkerId,
        factory: &ActorRef<FactoryMessage<String, TerminalMsg>>,
        Job { msg, key, .. }: Job<String, TerminalMsg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // Process terminal message (e.g., SendInput, Resize)
        Ok(key)
    }
}

struct TerminalWorkerBuilder;
impl WorkerBuilder<TerminalWorker, ()> for TerminalWorkerBuilder {
    fn build(&mut self, wid: usize) -> (TerminalWorker, ()) {
        (TerminalWorker, ())
    }
}

// Spawn factory
let factory_args = FactoryArguments::builder()
    .worker_builder(Box::new(TerminalWorkerBuilder))
    .queue(Default::default())  // Bounded queue via factory configuration
    .router(routing::KeyPersistentRouting::new())  // Same terminal_id → same worker
    .num_initial_workers(0)  // Spawn workers on-demand
    .max_queue_size(1000)  // ✅ Bound of 1000 messages
    .build();

let (factory_ref, _handle) = Actor::spawn(None, factory, factory_args).await?;

// Dispatch jobs
factory.cast(FactoryMessage::Dispatch(Job {
    key: "terminal:123".to_string(),
    msg: TerminalMsg::SendInput { input: "ls\n".to_string(), reply },
    options: JobOptions::default(),
    accepted: None,
})).await?;
```

**Benefits**:
- **Automatic worker restart**: Factory restarts crashed workers, preserving job queue
- **Backpressure via bounded queue**: Rejects or delays jobs when full
- **Worker isolation**: Each worker processes independently, no shared state

**2. Priority Queues for Message Types**

If you need prioritized message handling (e.g., signals vs. output), use `ractor::factory::queues::PriorityQueue`:

```rust
use ractor::factory::queues::{PriorityQueue, Priority};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
enum TerminalPriority {
    High,    // Stop, Kill signals
    Medium,   // Resize, Start
    Low,      // Regular input
}

impl Priority for TerminalPriority {
    fn priority(&self) -> u8 {
        match self {
            TerminalPriority::High => 0,
            TerminalPriority::Medium => 1,
            TerminalPriority::Low => 2,
        }
    }
}

// Configure factory with priority queue
let queue = PriorityQueue::new(3);  // 3 priority levels
```

### Monitoring Mailbox Pressure

Add observability to detect backpressure events:

```rust
impl Actor for DesktopSupervisor {
    // ...

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            DesktopSupervisorMsg::GetOrCreateDesktop { .. } => {
                // Track request rate
                state.request_counter += 1;

                // Check mailbox size (if available via ractor API)
                if let Ok(size) = myself.mailbox_size() {
                    if size > 500 {
                        tracing::warn!(
                            supervisor = %myself.get_id(),
                            mailbox_size = size,
                            "DesktopSupervisor experiencing backpressure"
                        );
                    }
                }

                // ... handle request
            }
        }
    }
}
```

---

## Failure Handling and Restart Semantics

### Supervision Event Types

From `ractor::actor::messages::SupervisionEvent`:

| Event | Variant | When Fired | Supervisor Action |
|--------|----------|-------------|------------------|
| `ActorStarted` | Child successfully spawned | Log startup; no action needed |
| `ActorTerminated` | Child stopped cleanly | Decide whether to restart based on exit reason |
| `ActorFailed` | Child panicked or returned error | **Restart child** (with same args or clean state) |
| `ProcessGroupChanged` | Process group membership changed | Update internal tracking |

### Restart Strategies

Based on Erlang/OTP `supervision` design:

| Strategy | Behavior | Use Case |
|----------|----------|-----------|
| **one_for_one** | Only failed actor is restarted | Most actors (DesktopActor, ChatActor, TerminalActor) |
| **rest_for_one** | Failed actor and all later-started siblings are terminated, then all restarted | Closely coupled actors (e.g., chat + its LLM agent) |
| **one_for_all** | All siblings terminated, then all restarted | **Not recommended**: causes excessive restarts |
| **simple_one_for_one** | Dynamic child pool (factory pattern) | TerminalWorker pool, API request handlers |

### Implementation Pattern

```rust
impl Actor for DesktopSupervisor {
    // ...

    async fn handle_supervision_event(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) {
        match event {
            SupervisionEvent::ActorStarted(actor_cell) => {
                let actor_id = actor_cell.get_id();
                tracing::info!(
                    supervisor = %myself.get_id(),
                    child = %actor_id,
                    "DesktopActor started"
                );

                // Track child state
                state.children.insert(actor_id, ChildInfo {
                    restart_count: 0,
                    last_started: tokio::time::Instant::now(),
                });
            }

            SupervisionEvent::ActorTerminated(actor_cell, last_state, exit_reason) => {
                let actor_id = actor_cell.get_id();

                // ✅ Decide based on exit reason
                match exit_reason {
                    Some(reason) if reason.contains("graceful_shutdown") => {
                        // Normal shutdown, don't restart
                        tracing::info!(
                            supervisor = %myself.get_id(),
                            child = %actor_id,
                            reason = %reason,
                            "DesktopActor terminated gracefully, not restarting"
                        );
                        state.children.remove(&actor_id);
                    }
                    Some(reason) => {
                        // Abnormal shutdown, restart
                        tracing::warn!(
                            supervisor = %myself.get_id(),
                            child = %actor_id,
                            reason = %reason,
                            "DesktopActor terminated abnormally, restarting"
                        );
                        self.restart_child(myself, actor_id, state).await;
                    }
                    None => {
                        // No reason provided, assume clean shutdown
                        tracing::info!(
                            supervisor = %myself.get_id(),
                            child = %actor_id,
                            "DesktopActor terminated (no reason), not restarting"
                        );
                        state.children.remove(&actor_id);
                    }
                }
            }

            SupervisionEvent::ActorFailed(actor_cell, error) => {
                let actor_id = actor_cell.get_id();

                tracing::error!(
                    supervisor = %myself.get_id(),
                    child = %actor_id,
                    error = %error,
                    "DesktopActor failed, restarting"
                );

                // ✅ Always restart on failure
                self.restart_child(myself, actor_id, state).await;
            }

            _ => {}
        }
    }

    async fn restart_child(
        &self,
        myself: ActorRef<Self::Msg>,
        child_id: ractor::ActorId,
        state: &mut Self::State,
    ) {
        // Get child info for restart
        if let Some(child_info) = state.children.get(&child_id) {
            let child_info = child_info.clone();
            child_info.restart_count += 1;

            // ✅ Implement restart intensity limit
            if child_info.restart_count > MAX_RESTARTS {
                tracing::error!(
                    supervisor = %myself.get_id(),
                    child = %child_id,
                    restart_count = child_info.restart_count,
                    "Restart intensity exceeded, stopping supervisor"
                );

                // Escalate to parent supervisor
                myself.stop(Some("Restart intensity exceeded".to_string()));
                return;
            }

            // ✅ Restart child with same configuration
            if let Some(args) = state.child_args.get(&child_id) {
                // Spawn new instance (will be linked automatically)
                let _ = Actor::spawn(
                    Some(child_id_to_name(&child_id)),
                    DesktopActor,
                    args.clone(),
                )
                .await
                .map_err(|e| {
                    tracing::error!(
                        supervisor = %myself.get_id(),
                        child = %child_id,
                        error = %e,
                        "Failed to restart DesktopActor"
                    );
                });

                tracing::info!(
                    supervisor = %myself.get_id(),
                    child = %child_id,
                    restart_count = child_info.restart_count,
                    "Restarted DesktopActor"
                );
            }
        }
    }
}

const MAX_RESTARTS: u32 = 3;  // Erlang/OTP default intensity
```

### Restart Intensity and Period

From Erlang/OTP supervision principles:

```rust
/// Restart intensity: maximum number of restarts within a time period
const RESTART_INTENSITY: u32 = 3;

/// Period: time window for intensity counting (seconds)
const RESTART_PERIOD: u64 = 5;  // Erlang/OTP default

/// Check if restart is allowed
fn should_restart_child(restart_count: u32, window_start: Instant) -> bool {
    let now = Instant::now();

    // Check if we're outside the time window
    if now.duration_since(window_start) > Duration::from_secs(RESTART_PERIOD) {
        return true;  // Reset counter on new window
    }

    // Within window: check intensity limit
    restart_count < RESTART_INTENSITY
}

impl Actor for DesktopSupervisor {
    // ...

    async fn handle_supervision_event(
        &self,
        myself: ActorRef<Self::Msg>,
        event: SupervisionEvent,
        state: &mut Self::State,
    ) {
        match event {
            SupervisionEvent::ActorFailed(..) | SupervisionEvent::ActorTerminated(..) => {
                // Check restart intensity before restarting
                if !should_restart_child(state.restart_count, state.restart_window_start) {
                    tracing::error!(
                        supervisor = %myself.get_id(),
                        "Restart intensity exceeded, giving up"
                    );
                    myself.stop(Some("Restart intensity exceeded".to_string()));
                    return Ok(());
                }

                // Reset counter if outside window
                if Instant::now().duration_since(state.restart_window_start)
                    > Duration::from_secs(RESTART_PERIOD)
                {
                    state.restart_count = 0;
                    state.restart_window_start = Instant::now();
                }

                // Increment and restart
                state.restart_count += 1;
                self.restart_child(myself, child_id, state).await;
            }
            _ => {}
        }
    }
}
```

### State Persistence on Restart

For actors that need to preserve state across restarts (e.g., ChatAgent with LLM context):

```rust
impl Actor for ChatAgent {
    type Msg = ChatAgentMsg;
    type State = AgentState;
    type Arguments = AgentArguments;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        // ✅ Load persisted state from EventStore
        let persisted_state = load_persisted_state(&args.agent_id).await?;

        Ok(AgentState {
            agent_id: args.agent_id,
            llm_context: persisted_state.unwrap_or_else(|| default_llm_context()),
            message_count: 0,
        })
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            ChatAgentMsg::ProcessMessage { msg, reply } => {
                state.message_count += 1;

                // Process with LLM
                let response = call_llm(&state.llm_context, &msg).await?;

                // Update local state
                state.llm_context = update_context(state.llm_context, &msg, &response);

                // ✅ Persist state to EventStore every N messages
                if state.message_count % 10 == 0 {
                    persist_state(&state.agent_id, &state.llm_context).await?;
                }

                let _ = reply.send(response);
            }
            _ => {}
        }
    }

    async fn post_stop(
        &self,
        myself: ActorRef<Self::Msg>,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        // ✅ Final state persistence before shutdown
        persist_state(&state.agent_id, &state.llm_context).await?;

        Ok(())
    }
}
```

---

## Request/Response and Fan-Out Patterns

### Request/Response (RPC)

**Current Pattern** (correct, but can be improved):
```rust
// actors/chat_agent.rs
let result = ractor::call!(llm_actor_ref, |reply| {
    LlmMsg::Generate {
        prompt,
        reply,
    }
}).await?;
```

**Best Practice**: Add timeouts to prevent indefinite blocking:

```rust
use tokio::time::{timeout, Duration};

let result = timeout(
    Duration::from_secs(30),
    ractor::call!(llm_actor_ref, |reply| {
        LlmMsg::Generate {
            prompt,
            reply,
        }
    })
).await;

match result {
    Ok(Ok(response)) => {
        // Process response
    }
    Ok(Err(ractor_err)) => {
        // RPC failed (actor dead, network error)
        tracing::error!("LLM RPC failed: {:?}", ractor_err);
    }
    Err(_) => {
        // Timeout - LLM took too long
        tracing::warn!("LLM request timed out");
        // Optionally retry or return error
    }
}
```

### Fan-Out (Broadcast to Multiple Actors)

**Use Process Groups (ractor::pg)** for dynamic actor discovery:

```rust
use ractor::pg;

// Subscribe actors to a process group
async fn join_process_group(
    actor_ref: ActorRef<MyActorMsg>,
    group_name: &str,
) {
    let group_name: ractor::GroupName::from(group_name.to_string());

    // Join group (creates process group if not exists)
    actor_ref.cast(MyActorMsg::JoinGroup {
        group_name: group_name.clone(),
    });

    // Wait for join confirmation
    tokio::time::sleep(Duration::from_millis(100)).await;
}

impl Actor for MyActor {
    // ...

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            MyActorMsg::JoinGroup { group_name } => {
                // ✅ Use ractor::pg to join
                if let Err(e) = pg::join(group_name, myself.clone()).await {
                    tracing::error!("Failed to join process group: {:?}", e);
                }
            }
            MyActorMsg::BroadcastToGroup {
                group_name,
                message,
            } => {
                // ✅ Broadcast to all members of group
                let group_name = ractor::GroupName::from(group_name.to_string());
                if let Ok(members) = pg::get_members(group_name).await {
                    for member in members {
                        // Send message to each member
                        if let Ok(actor_ref) = member.try_into::<ActorRef<MyActorMsg>>() {
                            let _ = actor_ref.cast(message.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
```

**Benefits of Process Groups:**
- Dynamic discovery: No need to maintain manual actor registries
- Automatic updates: New members are notified via `SupervisionEvent::ProcessGroupChanged`
- Decoupling: Senders don't need to know individual actor IDs

### Fan-In (Subscribe to Multiple Sources)

**Pattern**: Use multiple subscriptions and aggregate in actor state:

```rust
impl Actor for DesktopActor {
    type State = DesktopState {
        windows: HashMap<String, WindowState>,
        // ✅ Track active subscriptions for cleanup
        terminal_subscriptions: Vec<String>,
    };

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            DesktopActorMsg::OpenTerminalInWindow {
                window_id,
                terminal_id,
            } => {
                // ✅ Subscribe to terminal output
                let terminal_name = format!("terminal:{}", terminal_id);
                if let Some(terminal_ref) = registry::where_is(terminal_name).await {
                    // Store subscription for cleanup
                    state.terminal_subscriptions.push(terminal_name.clone());

                    // Subscribe to terminal output stream
                    terminal_ref.cast(TerminalMsg::SubscribeOutput {
                        reply: RpcReplyPort::new(),
                    });
                }
            }
            DesktopActorMsg::TerminalOutput {
                terminal_id,
                output,
            } => {
                // ✅ Update window with terminal output
                if let Some(window) = state.windows.get_mut(&window_id) {
                    window.append_output(output);
                }
            }
            DesktopActorMsg::CloseWindow { window_id } => {
                // ✅ Clean up subscriptions
                if let Some(terminal_id) = state.windows.get(&window_id).and_then(|w| w.terminal_id.clone()) {
                    state.terminal_subscriptions.retain(|t| t != &format!("terminal:{}", terminal_id));
                }
            }
            _ => {}
        }
    }
}
```

---

## Source Links

### Ractor Official Documentation

| Resource | URL | Annotation |
|----------|-----|-----------|
| **Ractor crate documentation** | https://docs.rs/ractor/latest/ractor/ | Core API: `Actor`, `ActorRef`, `ActorProcessingErr` |
| **Runtime semantics** | https://raw.githubusercontent.com/slawlor/ractor/main/docs/runtime-semantics.md | Message priority, supervision, mailbox guarantees |
| **Supervision events** | https://docs.rs/ractor/latest/ractor/actor/messages/enum.SupervisionEvent.html | `ActorStarted`, `ActorTerminated`, `ActorFailed` variants |
| **Registry module** | https://docs.rs/ractor/latest/ractor/registry/ | Named actor discovery, auto-registration, auto-unregistration |
| **Factory pattern** | https://docs.rs/ractor/latest/ractor/factory/ | Worker pools, routing strategies, queue management |
| **Process groups** | https://docs.rs/ractor/latest/ractor/pg/ | Dynamic actor discovery, fan-out/broadcast patterns |

### Erlang/OTP Supervision Principles

| Resource | URL | Annotation |
|----------|-----|-----------|
| **Supervisor behaviour** | https://erlang.org/doc/design_principles/sup_princ.html | Official OTP supervision design patterns |
| **gen_server behavior** | https://www.erlang.org/doc/man/gen_server.html | Core process pattern that ractor models after |
| **gen_supervisor docs** | https://www.erlang.org/doc/man/gen_supervisor.html | Supervisor implementation details |

### ChoirOS Specific References

| Resource | URL | Annotation |
|----------|-----|-----------|
| **actor_manager.rs (current)** | /Users/wiz/choiros-rs/sandbox/src/actor_manager.rs | Current implementation with anti-patterns |
| **terminal.rs** | /Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs | TerminalActor with PTY management |
| **desktop.rs** | /Users/wiz/choiros-rs/sandbox/src/actors/desktop.rs | DesktopActor with window state |
| **api/terminal.rs** | /Users/wiz/choiros-rs/sandbox/src/api/terminal.rs | WebSocket handler with manual actor management |
| **AGENTS.md** | /Users/wiz/choiros-rs/AGENTS.md | ChoirOS development guide and quick commands |

### Additional Reading

| Resource | URL | Annotation |
|----------|-----|-----------|
| **Let it crash - supervision in action** | https://www.youtube.com/watch?v=fK5zGp9kE8 | Conference talk on supervision trees (RustConf 2024) |
| **The Zen of Erlang** | https://www.erlang.org/doc/reference_manual/users_guide.html | High-level OTP design philosophy |
| **Ractor GitHub issues** | https://github.com/slawlor/ractor/issues | Real-world questions and solutions |

---

## Appendices

### A. Monitoring and Observability

**Metrics to track**:
1. **Actor lifecycle metrics**:
   - Spawn rate (actors/sec)
   - Crash rate (actors/sec)
   - Average restart time
2. **Mailbox metrics**:
   - Queue depth per actor
   - Message processing latency
   - Backpressure events
3. **Domain-specific metrics**:
   - Terminal: Active PTY count, PTY uptime
   - Chat: Messages/sec, LLM response time
   - Desktop: Window open/close rate

**Implementation**:
```rust
use ractor::time;

impl Actor for ApplicationSupervisor {
    // ...

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        let state = ApplicationState {
            domain_supervisors: HashMap::new(),
            event_store: args.clone(),
            metrics: Metrics::new(),
        };

        // ✅ Start periodic metrics reporting
        time::send_interval(
            tokio::time::Duration::from_secs(60),
            myself.clone(),
            || ApplicationSupervisorMsg::ReportMetrics,
        );

        Ok(state)
    }
}
```

### B. Testing Cheat Sheet

**Test helper functions** (`tests/common/mod.rs`):
```rust
use ractor::{Actor, ActorRef};
use std::time::Duration;

/// Spawn test supervisor with default configuration
pub async fn spawn_test_supervisor() -> (ActorRef<TestSupervisorMsg>, tokio::task::JoinHandle<()>) {
    // ...
}

/// Wait for specific supervision event
pub async fn wait_for_supervision_event(
    supervisor: &ActorRef<TestSupervisorMsg>,
    timeout: Duration,
) -> SupervisionEvent {
    // Subscribe to supervision events via channel or state check
    // Timeout if event not received
}

/// Crash actor (simulate panic)
pub fn crash_actor(actor_ref: ActorRef<TestActorMsg>) {
    actor_ref.kill();  // Immediate termination
}

/// Gracefully stop actor
pub async fn stop_actor_gracefully(actor_ref: ActorRef<TestActorMsg>) {
    actor_ref.stop(None);
    // Wait for ActorTerminated event
}
```

### C. Migration Checklist

**Phase 1 (Foundation)**:
- [ ] `supervisor.rs` module created
- [ ] `ApplicationSupervisor` implemented with `SupervisionEvent` handling
- [ ] Feature flag `supervision_refactor` added
- [ ] Unit tests for supervision events passing
- [ ] Documentation updated

**Phase 2 (Desktop)**:
- [ ] `DesktopSupervisor` implemented
- [ ] DesktopActor spawns named instances
- [ ] Routing from ApplicationSupervisor → DesktopSupervisor
- [ ] Backward compatibility with ActorManager maintained
- [ ] Integration tests for desktop restart scenarios

**Phase 3 (Terminal)**:
- [ ] `TerminalWorker` implements `Worker` trait
- [ ] `TerminalFactory` configured with bounded queue
- [ ] Routing key-persistent for terminal_id
- [ ] API handlers updated to use factory
- [ ] Stress tests for backpressure

**Phase 4 (Chat)**:
- [ ] `ChatSupervisor` implemented
- [ ] State persistence in ChatAgent/ChatActor
- [ ] EventStore integration for state loading
- [ ] Integration tests for message persistence

**Phase 5 (Cleanup)**:
- [ ] All API handlers use supervision tree
- [ ] ActorManager deprecated and documented
- [ ] DashMap/Mutex removed
- [ ] Full test suite passing
- [ ] E2E tests passing
- [ ] AGENTS.md updated

---

## Glossary

| Term | Definition |
|------|------------|
| **Actor** | Concurrent process with isolated state, communicating via messages |
| **Supervisor** | Actor that manages lifecycle of child actors via supervision events |
| **Supervision Tree** | Hierarchical structure of supervisors and workers |
| **ActorRef** | Strong reference to an actor, used for sending messages |
| **SupervisionEvent** | Message sent to supervisor when child lifecycle event occurs |
| **ActorCell** | Weak reference to an actor, used for type-erased operations |
| **Process Group (ractor::pg)** | Named group of actors for discovery and broadcast |
| **Factory** | Manager of worker pool with routing and queueing |
| **Registry** | Global named actor lookup, auto-registers on spawn |
| **Restart Intensity** | Maximum allowed restarts within a time period (Erlang/OTP default: 3 in 5 seconds) |
| **Restart Strategy** | How supervisor responds to child failures (`one_for_one`, `rest_for_one`, etc.) |
| **DashMap** | Concurrent hash map crate (external to ractor) |
| **Mutex** | Mutual exclusion primitive for synchronization |

---

**Document Version**: 1.0
**Last Updated**: 2025-02-06
**Author**: ChoirOS Architecture Research
**Status**: Ready for implementation
