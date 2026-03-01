# Handoff: Ractor EventBusActor Implementation Complete

**Created:** 2026-02-04  
**Status:** Ready for next phase  
**Previous:** Event bus design and implementation

---

## Summary

Successfully implemented the EventBusActor using ractor instead of Actix, establishing the pattern for the broader Actix→ractor migration. This is the first ractor actor in the codebase and serves as the foundation for the pub/sub event system.

---

## What Was Built

### Core Implementation

**`sandbox/src/actors/event_bus.rs`** (580 lines)
- `EventBusActor` - Main actor with ractor Process Groups integration
- `Event` / `EventType` - Core event types with serialization
- Topic-based pub/sub with wildcard support (`worker.*`)
- Integration with existing EventStoreActor (Actix bridge)
- Helper functions: `publish_event()`, `subscribe()`, `unsubscribe()`

**Key Design Patterns Established:**
```rust
// Process Groups for topic membership
ractor::pg::join(topic, vec![subscriber.get_cell()]);

// Manual broadcast (PG doesn't have built-in broadcast)
let members = ractor::pg::get_members(&topic);
for member in members {
    let actor_ref: ActorRef<Event> = member.into();
    ractor::cast!(actor_ref, event.clone())?;
}
```

### Tests

**`sandbox/src/actors/event_bus_test.rs`** (600+ lines)
- Unit tests for core functionality
- Mock EventStoreActor for isolation testing
- Test subscriber actor pattern
- Property-based test concepts
- Integration test structure

**Current test status:** 4 unit tests passing

### Documentation

**`docs/design/event_bus_ractor_design.md`** (400+ lines)
- Architecture overview
- Data types and message definitions
- Actor hierarchy diagrams
- Integration points (WebSocket, Workers)
- Testing strategy
- Migration path

---

## Files Changed

```
sandbox/Cargo.toml                    + ractor, async-trait, strum
sandbox/src/actors/event_bus.rs       + NEW: Main implementation
sandbox/src/actors/event_bus_test.rs  + NEW: Test suite
sandbox/src/actors/event_store.rs     + EventStoreMsg enum for bridge
sandbox/src/actors/mod.rs             + exports
```

---

## Architecture Decisions

### Why ractor Process Groups?

- **Membership tracking**: PG tracks which actors are in which groups (topics)
- **No built-in broadcast**: You get members, then send individually
- **Automatic cleanup**: Actors removed from PG on death
- **Erlang semantics**: Matches OTP pg module behavior

### Wildcard Pattern Strategy

```rust
// Subscribe to "worker.*" → receives "worker.task", "worker.job"
// Subscribe to "*" → receives all events
// Exact match "worker.task" → receives only that topic
```

### Persistence Bridge

EventBusActor (ractor) → EventStoreMsg::Append → EventStoreActor (Actix)

This allows gradual migration without breaking existing code.

---

## Next Steps

### Immediate (Priority 1)

1. **Complete EventBusActor tests**
   - Integration tests in `event_bus_test.rs`
   - Run full test suite: `cargo test -p sandbox event_bus`
   - Add property-based tests (proptest)

2. **Convert EventStoreActor to ractor**
   - Remove Actix dependency
   - Implement ractor::Actor trait
   - Update all references
   - Remove bridge code

### Short-term (Priority 2)

3. **WebSocket Integration**
   - Create WebSocketActor (ractor)
   - Bridge WebSocket connections to EventBus
   - Dashboard receives events via WebSocket

4. **TerminalActor for OpenCode**
   - New actor for PTY management
   - Integrate with DesktopActor
   - xterm.js frontend in Dioxus

### Medium-term (Priority 3)

5. **Migrate remaining actors**
   - ChatActor → ractor
   - ChatAgent → ractor  
   - DesktopActor → ractor

6. **Remove Actix completely**
   - Update Cargo.toml
   - Clean up all actix imports
   - Update tests

---

## Open Questions

1. **Event retention**: How long should events persist in EventStore?
2. **Backpressure**: What happens when subscribers can't keep up?
3. **Security**: Topic-level access control needed?
4. **Clustering**: Will we need ractor's cluster feature later?

---

## Testing Checklist

- [x] Unit tests for EventBusActor
- [ ] Integration tests (WebSocket flow)
- [ ] Load tests (10k events/sec)
- [ ] Property-based tests
- [ ] Agentic red teaming

---

## References

- Design doc: `docs/design/event_bus_ractor_design.md`
- Implementation: `sandbox/src/actors/event_bus.rs`
- Tests: `sandbox/src/actors/event_bus_test.rs`
- Ractor docs: https://docs.rs/ractor/latest/ractor/

---

**Recovery Principle:** Testing standards are STRICTER. The EventBusActor test suite should be expanded before moving to the next phase.

**Next Action:** Complete integration tests, then convert EventStoreActor to ractor.
