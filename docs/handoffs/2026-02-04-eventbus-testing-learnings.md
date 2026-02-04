# Handoff: EventBusActor Testing - Key Learnings

**Created:** 2026-02-04  
**Status:** Complete - All tests passing (17/17)  
**Previous:** EventBusActor implementation

---

## Summary

Successfully completed comprehensive testing for the EventBusActor. The testing process revealed several critical patterns for testing ractor-based actors that should be documented for future migrations.

---

## What Was Tested

### Test Suite Overview
- **17 tests total** (4 unit tests in event_bus.rs + 13 integration tests in event_bus_test.rs)
- **All tests passing**
- **Coverage areas:**
  - Actor lifecycle (start/stop)
  - Pub/sub functionality
  - Topic isolation
  - Wildcard matching
  - Event ordering
  - Unsubscribe behavior
  - High throughput (1000 events)
  - Concurrent subscribers (10 subscribers, 100 events each)
  - Error handling

---

## Critical Testing Patterns Discovered

### 1. **Anonymous Actors Required for Concurrent Tests**

**Problem:** Tests run concurrently and share the ractor registry. Named actors collide.

**Solution:** Use `None` instead of `Some("name")` for all actors in tests:

```rust
// WRONG - causes ActorAlreadyRegistered errors
let (bus_ref, _handle) = Actor::spawn(
    Some("test-bus".to_string()),  // ❌ Collides across tests
    EventBusActor,
    args,
).await?;

// CORRECT - anonymous actors
let (bus_ref, _handle) = Actor::spawn(
    None,  // ✅ Unique actor per test
    EventBusActor,
    args,
).await?;
```

---

### 2. **Process Groups Are Global - Use Unique Topics**

**Problem:** ractor's Process Groups (PG) are global across the entire process. Tests share the same PG namespace, causing cross-test contamination.

**Symptom:** Subscribers receive events from other tests (e.g., expected 1 event, received 8).

**Solution:** Generate unique topic names per test using an atomic counter:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

static TOPIC_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_topic(base: &str) -> String {
    format!("{}-{}", base, TOPIC_COUNTER.fetch_add(1, Ordering::SeqCst))
}

// In tests:
let topic = unique_topic("test.topic");  // "test.topic-0", "test.topic-1", etc.
```

**Key Insight:** This is different from Actix where each test actor system is isolated. Ractor uses a global PG namespace.

---

### 3. **Timing and Async Considerations**

**Pattern:** Always add small delays after subscribe/unsubscribe operations:

```rust
// Subscribe
ractor::cast!(bus_ref, EventBusMsg::Subscribe { topic, subscriber })?;
tokio::time::sleep(Duration::from_millis(100)).await;  // Let PG update

// Publish
ractor::cast!(bus_ref, EventBusMsg::Publish { event, persist: false })?;
tokio::time::sleep(Duration::from_millis(100)).await;  // Let event propagate

// Assert
assert_eq!(received.lock().await.len(), 1);
```

**Why:** Process Group operations are async and may not be immediately visible.

---

### 4. **Actor Status Check Timing**

**Problem:** Checking `actor.get_status()` immediately after spawn returns `Starting` instead of `Running`.

**Solution:** Add a small delay before status check:

```rust
let (bus_ref, bus_handle) = Actor::spawn(None, EventBusActor, args).await?;
tokio::time::sleep(Duration::from_millis(50)).await;
assert_eq!(bus_ref.get_status(), ractor::ActorStatus::Running);  // ✅
```

---

### 5. **Test Isolation Strategy**

For ractor tests, isolation requires:

1. **Anonymous actors** (no names)
2. **Unique topics** (atomic counter)
3. **Timing delays** (for PG propagation)
4. **Proper cleanup** (stop actors at end)

```rust
#[tokio::test]
async fn test_example() {
    // Setup with anonymous actors
    let (bus_ref, _bus_handle) = Actor::spawn(None, EventBusActor, args).await?;
    let (sub_ref, _sub_handle) = Actor::spawn(None, TestSubscriber, ()).await?;
    
    // Use unique topic
    let topic = unique_topic("my.topic");
    
    // Subscribe with delay
    ractor::cast!(bus_ref, EventBusMsg::Subscribe { topic: topic.clone(), subscriber: sub_ref })?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // Test...
    
    // Cleanup
    bus_ref.stop(None);
}
```

---

## Test Performance Results

- **1000 events throughput test:** ~2000-5000 events/sec (depending on load)
- **Concurrent subscribers (10x100):** All 1000 events delivered correctly
- **Test suite execution time:** ~0.6 seconds for 17 tests

---

## Files Changed

```
sandbox/src/actors/event_bus_test.rs    + Comprehensive test suite
sandbox/src/actors/event_bus.rs         + Made event_store optional for testing
sandbox/src/actors/mod.rs               + Added test module inclusion
```

---

## Migration Guidelines for Future Actors

When converting other actors to ractor:

1. **Use the `unique_topic()` pattern** for any topic-based messaging
2. **Use anonymous actors** in all tests
3. **Add timing delays** after PG operations
4. **Make external dependencies optional** (like event_store) for easier testing
5. **Document PG behavior** in actor documentation

---

## Next Steps

1. ✅ EventBusActor tests complete
2. 🔄 Convert EventStoreActor to ractor (remove Actix dependency)
3. ⏳ Create WebSocketActor for dashboard integration
4. ⏳ Create TerminalActor for opencode integration
5. ⏳ Migrate remaining actors (ChatActor, ChatAgent, DesktopActor)

---

## References

- Ractor Process Groups: https://docs.rs/ractor/latest/ractor/pg/index.html
- Test file: `sandbox/src/actors/event_bus_test.rs`
- Implementation: `sandbox/src/actors/event_bus.rs`

---

**Recovery Principle:** Testing ractor actors requires understanding global state (Process Groups). The `unique_topic()` pattern is essential for test isolation.
