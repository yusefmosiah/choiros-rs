# Handoff: EventStoreActor Migration to Ractor - In Progress

**Created:** 2026-02-04  
**Status:** In Progress - EventStoreActor converted, dependent files need updates  
**Previous:** EventBusActor testing completion

---

## Summary

Successfully converted EventStoreActor from Actix to ractor. The actor now uses ractor's Actor trait with RPC-based messaging. However, the migration is incomplete because many files depend on the old Actix-based API.

---

## What Was Completed

### EventStoreActor Conversion (✅)

**File:** `sandbox/src/actors/event_store.rs`

**Changes Made:**
1. Converted from Actix `Actor` trait to ractor `Actor` trait
2. Changed message types from Actix `Message` to ractor `EventStoreMsg` enum with `RpcReplyPort`
3. Updated error types to be `Clone` (required for ractor)
4. Added `EventStoreArguments` enum for spawn arguments
5. Implemented proper ractor lifecycle methods (`pre_start`, `post_start`, `handle`, `post_stop`)
6. Converted unit tests to use ractor patterns (anonymous actors, RPC calls)
7. Added helper functions: `append_event`, `get_events_for_actor`, `get_event_by_seq`

**New API:**
```rust
// Spawn actor
let (store_ref, _handle) = Actor::spawn(
    None,
    EventStoreActor,
    EventStoreArguments::InMemory,
).await?;

// Append event
let event = ractor::call!(store_ref, |reply| EventStoreMsg::Append {
    event: AppendEvent { ... },
    reply,
})?;

// Get events
let events = ractor::call!(store_ref, |reply| EventStoreMsg::GetEventsForActor {
    actor_id: "actor-1".to_string(),
    since_seq: 0,
    reply,
})?;
```

**Old API (no longer works):**
```rust
// Spawn actor
let store = EventStoreActor::new_in_memory().await.unwrap().start();

// Append event
let event = store.send(AppendEvent { ... }).await.unwrap().unwrap();

// Get events
let events = store.send(GetEventsForActor { ... }).await.unwrap().unwrap();
```

---

## Files That Need Updating

### High Priority (Blocking compilation)

1. **`sandbox/src/actor_manager.rs`**
   - Uses `Addr<EventStoreActor>` (Actix address type)
   - Needs to use `ActorRef<EventStoreMsg>` (ractor reference type)
   - Lines: 27, 31, 41, 131

2. **`sandbox/src/actors/chat.rs`**
   - Uses `Addr<EventStoreActor>` and `GetEventsForActor` message
   - Lines: 24, 33, 37, 93-122 (sync_with_event_store method)
   - Needs conversion to use ractor RPC calls

3. **`sandbox/src/actors/chat_agent.rs`**
   - Uses `Addr<EventStoreActor>`
   - Lines: 11, 23, 32

4. **`sandbox/src/actors/desktop.rs`**
   - Uses `Addr<EventStoreActor>` and `GetEventsForActor` message
   - Lines: 22, 33, 37

5. **`sandbox/src/api/chat.rs`**
   - Uses `GetEventsForActor` message
   - Line: 13

### Medium Priority (Tests)

6. **`sandbox/tests/persistence_test.rs`**
   - Extensive use of old API throughout
   - ~50+ usages of `EventStoreActor::new_in_memory().await.unwrap().start()`
   - All need conversion to ractor spawn pattern

7. **`sandbox/tests/desktop_api_test.rs`**
   - Uses old API
   - Line: 26

8. **`sandbox/tests/websocket_chat_test.rs`**
   - Uses old API
   - Line: 87

---

## Migration Strategy Options

### Option 1: Update All Files (Recommended)
Convert all dependent files to use ractor. This is the cleanest approach but requires significant work.

**Pros:**
- Clean, consistent codebase
- All actors use same framework
- No compatibility layers needed

**Cons:**
- Large amount of code to change
- Risk of introducing bugs
- Time-intensive

### Option 2: Create Compatibility Layer
Create an Actix wrapper around the ractor EventStoreActor to maintain backward compatibility temporarily.

**Pros:**
- Minimal changes to existing code
- Can migrate gradually

**Cons:**
- Adds complexity
- Two actor frameworks running simultaneously
- Technical debt

### Option 3: Revert and Plan Better
Revert EventStoreActor to Actix and plan a coordinated migration of all actors together.

**Pros:**
- Code compiles and works
- Can plan comprehensive migration

**Cons:**
- Delaying the migration
- Still need to do the work eventually

---

## Recommended Next Steps

1. **Choose migration strategy** (Option 1 recommended)
2. **Update ActorManager** to use ractor references
3. **Update ChatActor** to use ractor RPC for EventStore
4. **Update ChatAgent** to use ractor RPC for EventStore
5. **Update DesktopActor** to use ractor RPC for EventStore
6. **Update API layer** (api/chat.rs)
7. **Update all tests**
8. **Run full test suite** to verify everything works

---

## Key Patterns for Migration

### Pattern 1: Replace Actix Addr with Ractor ActorRef

**Before:**
```rust
use actix::Addr;

struct MyActor {
    event_store: Option<Addr<EventStoreActor>>,
}
```

**After:**
```rust
use ractor::ActorRef;

struct MyActor {
    event_store: Option<ActorRef<EventStoreMsg>>,
}
```

### Pattern 2: Replace Actix send() with Ractor RPC

**Before:**
```rust
let events = event_store
    .send(GetEventsForActor { actor_id, since_seq })
    .await
    .unwrap()
    .unwrap();
```

**After:**
```rust
let events = ractor::call!(event_store, |reply| EventStoreMsg::GetEventsForActor {
    actor_id,
    since_seq,
    reply,
})?
.unwrap();
```

### Pattern 3: Replace Actix spawn with Ractor spawn

**Before:**
```rust
let store = EventStoreActor::new_in_memory().await.unwrap().start();
```

**After:**
```rust
let (store_ref, _handle) = Actor::spawn(
    None,
    EventStoreActor,
    EventStoreArguments::InMemory,
).await?;
```

---

## Files Changed So Far

```
sandbox/src/actors/event_store.rs    + Complete rewrite to ractor
sandbox/src/actors/event_bus.rs      + Updated persistence call
sandbox/src/actors/mod.rs            + Added EventStoreArguments export
```

---

## Testing Status

- ✅ EventStoreActor unit tests (3 tests) - Written for ractor
- ⏳ Integration tests - Need to be updated
- ⏳ Compilation - Blocked by dependent files

---

## References

- Ractor documentation: https://docs.rs/ractor/latest/ractor/
- EventBusActor (reference implementation): `sandbox/src/actors/event_bus.rs`
- Previous handoff: `docs/handoffs/2026-02-04-eventbus-testing-learnings.md`

---

**Recovery Principle:** When migrating actors between frameworks, update all dependent code atomically or provide a compatibility layer. The EventStoreActor is a core dependency used throughout the codebase.
