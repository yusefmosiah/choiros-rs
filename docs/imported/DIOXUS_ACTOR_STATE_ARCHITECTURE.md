# ChoirOS + Dioxus: Actor-State-UI Architecture

**Core Principle:** State lives in the Actor (SQLite), UI is a reactive projection.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│  User Browser (Dioxus WASM)                                │
│  ┌───────────────────────────────────────────────────────┐ │
│  │ ChatApp Component                                     │ │
│  │                                                       │ │
│  │  let state = use_resource(|| async {                 │ │
│  │      call_actor("chat", "get_messages").await        │ │
│  │  });                                                  │ │
│  │                                                       │ │
│  │  rsx! {                                               │ │
│  │      for msg in state() {                            │ │
│  │          Message { content: msg.content }            │ │
│  │      }                                                │ │
│  │  }                                                    │ │
│  └──────────────────┬────────────────────────────────────┘ │
│                     │                                       │
│  ┌──────────────────▼────────────────────────────────────┐ │
│  │ UserApp Component (Hot-swappable)                     │ │
│  │ - Generated from LLM prompts                          │ │
│  │ - Can be recompiled and swapped at runtime            │ │
│  │ - Connects to same ChatActor via actor_id            │ │
│  └───────────────────────────────────────────────────────┘ │
└──────────────────────┬──────────────────────────────────────┘
                       │ HTTP/WebSocket
                       ▼
┌─────────────────────────────────────────────────────────────┐
│  Sandbox (Actix Web Server)                                │
│  ┌───────────────────────────────────────────────────────┐ │
│  │ ChatActor                                             │ │
│  │                                                       │ │
│  │  struct ChatActor {                                  │ │
│  │      actor_id: String,                               │ │
│  │      event_store: Addr<EventStoreActor>,             │ │
│  │  }                                                    │ │
│  │                                                       │ │
│  │  impl Actor for ChatActor {                          │ │
│  │      fn handle(GetMessages, ctx) -> Vec<Message> {   │ │
│  │          // Query SQLite                              │ │
│  │          // Return current state                      │ │
│  │      }                                                │ │
│  │  }                                                    │ │
│  └──────────────────┬────────────────────────────────────┘ │
│                     │                                       │
│  ┌──────────────────▼────────────────────────────────────┐ │
│  │ EventStoreActor (SQLite)                              │ │
│  │                                                       │ │
│  │  CREATE TABLE events (                               │ │
│  │      seq INTEGER PRIMARY KEY,                        │ │
│  │      actor_id TEXT,                                  │ │
│  │      event_type TEXT,                                │ │
│  │      payload JSON,                                   │ │
│  │      timestamp DATETIME                              │ │
│  │  );                                                   │ │
│  │                                                       │ │
│  │  CREATE TABLE actor_state (                          │ │
│  │      actor_id TEXT PRIMARY KEY,                      │ │
│  │      state_json JSON,                                │ │
│  │      version INTEGER                                 │ │
│  │  );                                                   │ │
│  └───────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

**Key insight:** The UI doesn't own state. It queries the actor, which owns state in SQLite.

---

## Hot Reload / Live Reprogramming

When user prompts "Make the chat UI look like iMessage":

```rust
// 1. LLM generates new component code
let new_component_code = llm.generate(
    "Create ChatApp component with iMessage styling",
    current_context
).await;

// 2. Compile to WASM module
let wasm_bytes = compile_to_wasm(&new_component_code).await;

// 3. Hot swap in running app
app.swap_component("ChatApp", wasm_bytes);

// 4. New component connects to same actor
// State preserved because it's in SQLite, not in component
```

**Implementation approaches:**

### Approach A: Component Registry (Recommended)

```rust
// Host app maintains component registry
static COMPONENT_REGISTRY: Mutex<HashMap<String, Box<dyn Component>>> = 
    Mutex::new(HashMap::new());

fn main() {
    dioxus::launch(|| {
        rsx! {
            DynamicComponents {}
        }
    });
}

#[component]
fn DynamicComponents() -> Element {
    let registry = use_hook(|| COMPONENT_REGISTRY.lock().unwrap());
    
    rsx! {
        for (name, component) in registry.iter() {
            {component.render()}
        }
    }
}

// Hot reload function
pub fn hot_reload_component(name: &str, wasm_bytes: Vec<u8>) {
    let component = load_wasm_component(wasm_bytes);
    let mut registry = COMPONENT_REGISTRY.lock().unwrap();
    registry.insert(name.to_string(), component);
    
    // Trigger re-render
    dioxus::signals::global::GLOBAL.update();
}
```

### Approach B: WebAssembly Dynamic Linking

```rust
// Each user app compiled as wasm32-unknown-unknown target
// Exported functions that host calls

#[no_mangle]
pub extern "C" fn render(actor_id: &str) -> String {
    // Component renders based on actor_id
    // Queries actor state via host-provided function
    let messages = host::query_actor(actor_id, "get_messages");
    render_html(messages)
}

#[no_mangle]
pub extern "C" fn handle_event(actor_id: &str, event: &str) {
    // Handle user interactions
    host::send_to_actor(actor_id, event);
}
```

Host provides:
- `host::query_actor()` - Read actor state
- `host::send_to_actor()` - Send events to actor
- `host::subscribe_to_actor()` - Subscribe to state changes

---

## State Flow

```
User Action
    ↓
Dioxus Component Event Handler
    ↓
POST /actor/{actor_id}/send
    ↓
Actix Handler
    ↓
ChatActor.handle(msg)
    ↓
EventStoreActor.append(event)
    ↓
SQLite (persisted)
    ↓
Event broadcast to subscribers
    ↓
Dioxus use_resource() sees update
    ↓
Component re-renders
```

**Critical:** State is never in the component. Component is pure function:
```rust
fn ChatComponent(actor_id: String) -> Element {
    // Read-only query to actor
    let state = use_resource(move || {
        let id = actor_id.clone();
        async move {
            query_actor(&id, "get_state").await
        }
    });
    
    rsx! {
        for msg in state().unwrap_or_default() {
            MessageView { msg }
        }
    }
}
```

---

## Why This Works for "Automatic Computer"

1. **State durability** - SQLite survives crashes, restarts, hot reloads
2. **Audit trail** - Event log shows every change (prompt → code → state change)
3. **Time travel** - Can rebuild state at any point from event log
4. **Reproducibility** - Given event log, can reconstruct exact UI state
5. **Safe experiments** - Fork actor state, experiment, rollback if needed

---

## Implementation Plan

### Phase 0: Foundation (30 min)
- Setup Dioxus workspace with Actix integration
- Create shared types (ActorId, Event, ActorState)
- Setup SQLite with sqlx

### Phase 1: Actor System (1 hour)
- EventStoreActor (append-only log)
- ChatActor (example app actor)
- Actor registry (spawn/kill actors)

### Phase 2: Dioxus UI (1 hour)
- Basic component that queries actor
- use_resource for polling
- Server functions to call actors

### Phase 3: Hot Reload (2 hours)
- Component registry
- WASM compilation pipeline
- Live swap mechanism

### Phase 4: Chat MVP (2 hours)
- Full chat app with actor-owned state
- Tool calling (bash, file ops)
- Hot reload demo: change UI style via prompt

---

## Comparison to React Approach

| Aspect | React + Vite HMR | Dioxus + Actor State |
|--------|------------------|---------------------|
| State location | React state / Context | Actor SQLite |
| Persistence | localStorage / API calls | Automatic (SQLite) |
| Hot reload | Vite HMR (state lost) | Component swap (state persists) |
| Time travel | Redux devtools only | Full event log replay |
| Multi-user | Complex sync | Natural (each user = actor) |
| Self-modify | Complex (eval risk) | WASM sandbox |

---

## Technical Details

### Dioxus use_resource Pattern

```rust
use dioxus::prelude::*;
use std::time::Duration;

fn ChatView(actor_id: String) -> Element {
    // Poll actor every 100ms for updates
    let state = use_resource(
        use_referrer!(actor_id),
        Duration::from_millis(100),
        move || {
            let id = actor_id.clone();
            async move {
                query_actor_state(&id).await
            }
        }
    );
    
    // Alternative: WebSocket push
    let ws_state = use_ws_subscription(&format!("ws://localhost:8001/actor/{}/events", actor_id));
    
    rsx! {
        div {
            for msg in state().unwrap_or_default() {
                Message { msg }
            }
        }
    }
}
```

### Actor Message Handlers

```rust
impl Handler<GetMessages> for ChatActor {
    type Result = Vec<ChatMessage>;
    
    fn handle(&mut self, _msg: GetMessages, _ctx: &mut Context<Self>) -> Self::Result {
        // Read from SQLite projection
        sqlx::query_as::<_, ChatMessage>(
            "SELECT * FROM chat_messages WHERE actor_id = ? ORDER BY timestamp"
        )
        .bind(&self.actor_id)
        .fetch_all(&self.db_pool)
        .await
        .unwrap_or_default()
    }
}

impl Handler<SendMessage> for ChatActor {
    type Result = ();
    
    fn handle(&mut self, msg: SendMessage, ctx: &mut Context<Self>) {
        // Append event
        self.event_store.do_send(AppendEvent {
            event_type: "chat.user_msg".to_string(),
            payload: serde_json::to_value(&msg).unwrap(),
            actor_id: self.actor_id.clone(),
        });
        
        // Maybe trigger LLM response
        if msg.text.starts_with("/") {
            self.handle_command(&msg.text);
        }
    }
}
```

### Component Hot Swap

```rust
// Global component storage
static ACTIVE_COMPONENTS: RwLock<HashMap<String, Box<dyn Any>>> = 
    RwLock::new(HashMap::new());

pub fn mount_component(name: &str, wasm: &[u8]) {
    let component = wasm::load_component(wasm);
    let mut components = ACTIVE_COMPONENTS.write().unwrap();
    components.insert(name.to_string(), Box::new(component));
}

// In Dioxus root
fn App() -> Element {
    let components = use_hook(|| ACTIVE_COMPONENTS.read().unwrap());
    
    rsx! {
        Router {
            Route { to: "/", Home {} }
            Route { to: "/app/:id", DynamicApp { id: id } }
        }
    }
}

#[component]
fn DynamicApp(id: String) -> Element {
    let components = ACTIVE_COMPONENTS.read().unwrap();
    
    if let Some(component) = components.get(&id) {
        component.render()
    } else {
        rsx! { "App not found" }
    }
}
```

---

## Open Questions

1. **WASM compilation speed** - Can we compile user-generated components fast enough?
2. **WASM bundle size** - Each component is a WASM module, how to share common deps?
3. **Dioxus fullstack** - Should we use Dioxus fullstack (server-side rendering) or separate Actix?

**My recommendation:** Start with separate Actix + Dioxus web (WASM) approach. It's proven and gives us control over the actor system.

Ready to prototype this architecture?