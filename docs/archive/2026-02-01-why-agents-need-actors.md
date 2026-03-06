# Why Agents Need Actors: A Dev Blog

**Date:** 2026-02-01  
**Author:** ChoirOS Team  
**Status:** Working hypothesis

---

## The Problem We Just Hit

I just watched a supervisor task burn 200+ tool calls waiting for 6 parallel subagents to finish doc analysis. Each subagent was doing exploration work - reading files, searching code, compiling reports. The supervisor sat blocked, consuming context window, doing nothing.

This is wrong.

In actor terms: the supervisor sent synchronous messages and blocked on the response. In a proper actor system, it would have sent async messages, returned immediately, and collected results via the event bus when workers completed.

The OpenCode SDK forces this blocking model. But our architecture shouldn't.

---

## From Position Paper to Practice

Our [position paper](../automatic_computer_position_paper.md) argues for the "automatic computer" - not a chatbot, but a computational environment that:

1. **Observes** user actions as events
2. **Processes** in background without explicit invocation  
3. **Responds** through existing channels
4. **Maintains** persistent state across sessions
5. **Operates** at all time scales simultaneously

This is pansynchronous computing. And it requires the actor model.

---

## Why Actors Solve This

### The Chatbot Trap

Current AI tools are chatbots:
- User speaks → AI responds → User waits
- Synchronous, stateless, unscalable
- Context exists only within the session

This works for simple queries. It fails for complex work:

| Task | Why chatbot fails |
|------|-------------------|
| Deep research | Takes hours, user can't wait |
| Multi-step projects | Context lost between sessions |
| Background monitoring | No trigger without user initiation |
| Parallel analysis | Sequential blocking wastes time |

### The Actor Alternative

Actors give us:

**Non-blocking messages** - Fire and forget. The supervisor sends work to workers and immediately continues. No waiting.

**State isolation** - Each actor owns its state. Workers don't corrupt supervisor state. Supervisor doesn't need to track worker internals.

**Supervision trees** - Actors can spawn/monitor other actors. If a worker crashes, the supervisor knows and can restart or escalate.

**Location transparency** - Don't care if workers run in the same process, different threads, or different machines. Messages work the same.

---

## What Just Happened (The Wrong Way)

```rust
// What we did (blocking - WRONG)
let result1 = task("analyze ARCHITECTURE_SPEC").await;     // BLOCKS 3 min
let result2 = task("analyze TESTING_STRATEGY").await;      // BLOCKS 3 min
let result3 = task("analyze MULTI_AGENT_VISION").await;    // BLOCKS 3 min
// ... etc
// Total: 18 minutes of blocked supervisor
```

The supervisor spawned 6 workers but waited for each to complete. It consumed 200+ tool calls doing nothing but waiting. The context window filled with idle chatter.

---

## What Should Happen (The Actor Way)

```rust
// What we should do (async - CORRECT)
let workers = vec![
    spawn_actor("doc-analyzer", "ARCHITECTURE_SPEC"),
    spawn_actor("doc-analyzer", "TESTING_STRATEGY"),
    spawn_actor("doc-analyzer", "MULTI_AGENT_VISION"),
    // ... etc
];

// Supervisor returns immediately, does other work
// Workers process in parallel
// Results arrive via event bus

for event in event_bus.subscribe("doc-analysis.complete") {
    results.push(event.payload);
    if results.len() == workers.len() {
        compile_report(results);
        break;
    }
}
```

The supervisor spawns workers and returns. Workers run in parallel. Results flow back through the event bus. The supervisor collects them asynchronously.

Total supervisor tool calls: ~10 (spawn + collect)  
Total time: 3 minutes (parallel, not sequential)  
Context window: Minimal

---

## The Technical Requirements

For this to work, we need:

### 1. Event Bus (Pub/Sub)

Not just an event log - a broadcast mechanism:

```rust
// EventStoreActor today (log only)
event_store.append(event).await;  // Stored, not broadcast

// What we need (broadcast)
event_bus.publish("doc-analysis.complete", result).await;
// Multiple subscribers receive the event
```

### 2. Actor Lifecycle Management

Spawn, monitor, restart:

```rust
let worker = ctx.spawn_anonymous(|_| DocAnalyzer);
ctx.watch(worker, |msg| match msg {
    WorkerResult::Success(data) => handle_success(data),
    WorkerResult::Failure(err) => handle_failure(err),
});
```

### 3. Non-blocking I/O

All external operations (LLM calls, file I/O) must be async:

```rust
// Bad - blocks the actor
let result = blocking_llm_call(prompt);

// Good - yields control
let result = async_llm_call(prompt).await;
```

### 4. Mailbox Backpressure

When workers are overwhelmed, messages should queue or drop, not crash the system:

```rust
// Bounded mailbox - backpressure
actor.send(msg).await?;  // May wait if mailbox full
// OR
actor.try_send(msg)?;    // May fail if mailbox full
```

---

## What We Have Today

Our current ChoirOS implementation has pieces:

**✅ Event sourcing** - EventStoreActor with sqlx-backed SQLite  
**✅ Actor system** - Actix with ChatActor, DesktopActor, etc.  
**✅ State isolation** - Each actor owns its SQLite state  
**❌ Broadcast bus** - Events are stored, not published  
**❌ Async supervision** - No spawn-and-monitor pattern  
**❌ Non-blocking SDK** - OpenCode sessions block

The gap: our actors are good for internal state management, but we can't yet spawn async workers that report back via events.

---

## The Zombie Process Problem

Other coding agents (Claude Code, etc.) do have background tasks. You can spawn a test server and it runs... and runs... and runs.

Three days later you discover it's still consuming resources.

### Background Tasks Without Supervision

The pattern:
1. Start a background task (test server, build process, etc.)
2. It runs async - good!
3. No visibility into whether it completed, failed, or is still running
4. No automatic cleanup
5. Zombie processes accumulate

This is async without actors. You get concurrency but lose observability.

### Actors Provide Supervision

With actors:
- **Spawn** - Worker starts with a unique ID
- **Monitor** - Parent watches worker lifecycle
- **Event stream** - All state changes are events
- **Cleanup** - Parent receives termination signal, can restart or clean up

```rust
// Worker signals lifecycle
event_bus.publish("worker.spawned", { id, task, timestamp });
// ... work happens ...
event_bus.publish("worker.completed", { id, result, duration });
// OR
event_bus.publish("worker.failed", { id, error, stack_trace });

// Supervisor sees all of this
```

No zombies. No mystery processes. Full observability.

### The Actor Advantage

| Feature | Chatbots | Background Tasks | Actors |
|---------|----------|------------------|---------|
| Async execution | ❌ | ✅ | ✅ |
| State visibility | ❌ | ❌ | ✅ |
| Automatic cleanup | ❌ | ❌ | ✅ |
| Failure detection | ❌ | ❌ | ✅ |
| Restart logic | ❌ | ❌ | ✅ |

Background tasks give you half the solution. Actors give you the complete solution: async + observable + supervised.

---

## The Path Forward

### Phase 1: Event Bus

Add broadcast capability to EventStoreActor:

```rust
pub struct EventBusActor {
    subscribers: HashMap<String, Vec<Recipient<Event>>>,
}

impl EventBusActor {
    pub fn subscribe(&mut self, topic: String, subscriber: Recipient<Event>);
    pub fn publish(&self, topic: String, event: Event);
}
```

### Phase 2: Async Worker Spawning

Create a WorkerActor that can spawn child actors and monitor them:

```rust
pub struct SupervisorActor {
    workers: HashMap<ActorId, WorkerHandle>,
    pending_results: Vec<WorkResult>,
}

impl SupervisorActor {
    pub async fn spawn_worker(&mut self, task: Task) -> ActorId;
    pub async fn collect_results(&mut self) -> Vec<WorkResult>;
}
```

### Phase 3: Non-blocking SDK Integration

Either:
- Extend OpenCode SDK with async session support
- Build our own lightweight agent runtime
- Use actorcode skill as the bridge (current approach)

---

## The Rule: Supervisors Never Block

From this experience, a new rule for our system:

> **Supervisors must never spawn blocking tasks.**
>
> Supervisors coordinate. Workers execute. If a supervisor blocks, it's a worker.

This goes in AGENTS.md as a hard constraint.

---

## Why This Matters

The automatic computer isn't just about background processing. It's about **compositional computing** - building complex behaviors from simple, async components.

Without actors:
- Parallel work is painful (blocking, sequential)
- Failure handling is manual (no supervision trees)
- State management is fragile (shared mutable state)
- Scaling is impossible (tight coupling)

With actors:
- Parallel work is natural (spawn and collect)
- Failure handling is structured (let it crash, restart)
- State management is safe (isolated, message-passing)
- Scaling is transparent (location independence)

---

## Conclusion

The 200-tool-call blocking incident was a symptom. The disease: trying to build async behavior with sync primitives.

Actors are the cure. Event sourcing + pub/sub + supervision = the automatic computer.

Our hypothesis: **The automatic computer must be an actor model.** Event sourcing with pub/sub and a control plane is actor enough.

Next step: Build the event bus. Then the async worker spawning. Then watch the automatic computer come alive.

---

## References

- [Position Paper: The Automatic Computer](../automatic_computer_position_paper.md)
- [Architecture Specification](../../ARCHITECTURE_SPECIFICATION.md)
- [Multi-Agent Vision](../../CHOIR_MULTI_AGENT_VISION.md)
- [Actor Model](https://en.wikipedia.org/wiki/Actor_model) - Wikipedia
- [Actix](https://actix.rs/) - Rust actor framework

---

*Last updated: 2026-02-01*
