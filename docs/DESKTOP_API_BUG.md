# Desktop API Bug Analysis

**Date:** 2026-02-02  
**Component:** `sandbox/src/actors/desktop.rs`  
**Affected Endpoint:** `GET /desktop/{desktop_id}`

## Problem

The `/desktop/{desktop_id}` endpoint returns empty state on first request after actor creation.

### Current Behavior

1. First HTTP request triggers `ActorManager::get_or_create_desktop()`
2. New `DesktopActor` is spawned
3. `Actor::started()` is called:
   ```rust
   fn started(&mut self, ctx: &mut Self::Context) {
       // Spawns async future, doesn't block
       self.sync_with_event_store(ctx);
       
       // Check happens IMMEDIATELY, before events load
       if self.apps.is_empty() {
           self.apps.insert("chat".to_string(), ...);
       }
   }
   ```
4. `GetDesktopState` message is sent and returns empty `windows` and `apps`
5. **Later**, the async event loading completes (too late)

### Root Cause

`sync_with_event_store()` spawns an `ActorFuture` but doesn't wait for completion:

```rust
fn sync_with_event_store(&mut self, ctx: &mut Context<Self>) {
    if let Some(event_store) = self.event_store.clone() {
        let fut = async move {
            // Query events...
        };
        
        // Spawned but NOT awaited - "fire and forget"
        ctx.spawn(fut.into_actor(self).map(|events, actor, _| {
            if let Some(events) = events {
                actor.project_events(events);
            }
        }));
    }
}
```

The `started()` method returns immediately, leaving the actor in an uninitialized state.

## Fix Options

### Option A: Block in `started()`

Change `sync_with_event_store()` to block until events are loaded.

**Pros:**
- Simple, fixes the race condition completely

**Cons:**
- Blocks the actor's mailbox thread
- Violates Actix best practices (actors shouldn't block in `started()`)
- Could delay other actors if event store is slow

### Option B: `initialized` flag with message queue

Add an `initialized: bool` field. Messages sent before initialization complete are queued.

```rust
pub struct DesktopActor {
    // ... existing fields
    initialized: bool,
    pending_messages: Vec<Box<dyn ActorMessage>>,
}

impl Handler<GetDesktopState> for DesktopActor {
    fn handle(&mut self, msg: GetDesktopState, ctx: &mut Context<Self>) -> Self::Result {
        if !self.initialized {
            // Either queue or trigger sync now
            return Box::pin(self.load_and_then(msg, ctx));
        }
        // Normal handling...
    }
}
```

**Pros:**
- Doesn't block actor creation
- Handles race condition transparently

**Cons:**
- More complex - need to buffer/wrap all message types
- First request latency increases

### Option C: Pre-warm actors at startup

Create DesktopActors eagerly at server startup instead of on first request.

```rust
// In main.rs or ActorManager
pub async fn prewarm_desktops(&self) {
    // Query distinct desktop_ids from event store
    // Create actor for each, wait for init
}
```

**Pros:**
- First request is fast
- Separation of concerns (startup vs runtime)

**Cons:**
- Memory overhead for inactive desktops
- Doesn't handle NEW desktops (still has race)

### Option D: Lazy sync in message handlers

Remove `sync_with_event_store()` from `started()`. Do it on first message instead.

```rust
impl Handler<GetDesktopState> for DesktopActor {
    fn handle(&mut self, _msg: GetDesktopState, _ctx: &mut Context<Self>) -> Self::Result {
        // If not synced yet, do it now (async)
        if self.last_seq == 0 {
            return Box::pin(self.sync_then_respond());
        }
        // Normal handling...
    }
}
```

**Pros:**
- No race condition - sync happens before response
- Simple to implement

**Cons:**
- First request waits for sync
- Need to handle concurrent requests during sync

## Recommendation

**Option D** is preferred for ChoirOS because:

1. Matches actor model semantics - actors respond to messages
2. First-request sync is acceptable for a desktop initialization
3. Simpler than Option B's message queueing
4. Doesn't require pre-warming (Option C)

## Implementation Sketch

```rust
impl Handler<GetDesktopState> for DesktopActor {
    type Result = actix::ResponseActFuture<Self, shared_types::DesktopState>;
    
    fn handle(&mut self, _msg: GetDesktopState, _ctx: &mut Context<Self>) -> Self::Result {
        if self.last_seq == 0 && self.event_store.is_some() {
            // First call - need to sync
            let event_store = self.event_store.clone().unwrap();
            let desktop_id = self.desktop_id.clone();
            
            Box::pin(async move {
                let events = event_store
                    .send(GetEventsForActor { actor_id: desktop_id, since_seq: 0 })
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                    
                shared_types::DesktopState {
                    windows: /* project from events */,
                    active_window: /* from events */,
                    apps: /* from events + defaults */,
                }
            }.into_actor(self))
        } else {
            // Already synced - return cached state
            let windows: Vec<_> = self.windows.values().cloned().collect();
            let active_window = self.active_window.clone();
            let apps: Vec<_> = self.apps.values().cloned().collect();
            
            Box::pin(async move {
                shared_types::DesktopState { windows, active_window, apps }
            }.into_actor(self))
        }
    }
}
```

## Related Files

- `sandbox/src/actors/desktop.rs` - DesktopActor implementation
- `sandbox/src/api/desktop.rs` - HTTP endpoint (calls `GetDesktopState`)
- `sandbox/src/actor_manager.rs` - Actor lifecycle management

## Verification

After fix, this curl should return valid JSON with `windows` and `apps` arrays:

```bash
curl http://localhost:8080/desktop/test-desktop
```

Current broken response: `{"success":true,"desktop":{"windows":[],"active_window":null,"apps":[]}}`

Expected response: `{"success":true,"desktop":{"windows":[...],"active_window":"...","apps":[...]}}`
