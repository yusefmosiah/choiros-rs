# The Automatic Computer: Architecture

**Status:** Design Document  
**Date:** 2026-02-01  
**Purpose:** Define the ChoirOS automatic computer architecture

---

## Core Thesis

> The Model is the Kernel. The Chatbot is the CLI. The Automatic Computer is the Personal Mainframe.

Current AI tools are chatbots: synchronous, blocking, stateless. The automatic computer is infrastructure: asynchronous, observable, persistent.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     USER INTERFACE                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Prompt Bar  │  │  App Windows│  │   Dashboard         │  │
│  │ (shell)     │  │  (tmux)     │  │   (observability)   │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
└─────────┼────────────────┼────────────────────┼─────────────┘
          │                │                    │
          ▼                ▼                    ▼
┌─────────────────────────────────────────────────────────────┐
│                    EVENT BUS (Pub/Sub)                       │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  Topics: user.input, worker.spawned, worker.complete,  │  │
│  │          findings.new, chat.message, file.changed      │  │
│  └────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
          │                │                    │
          ▼                ▼                    ▼
┌─────────────────────────────────────────────────────────────┐
│                    SUPERVISOR LAYER                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Desktop     │  │  Chat       │  │   Research          │  │
│  │ Supervisor  │  │  Supervisor │  │   Supervisor        │  │
│  │ (windows)   │  │  (sessions) │  │   (background)      │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
└─────────┼────────────────┼────────────────────┼─────────────┘
          │                │                    │
          ▼                ▼                    ▼
┌─────────────────────────────────────────────────────────────┐
│                      WORKER LAYER                            │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌────────┐ │
│  │ Pico    │ │ Nano    │ │ Micro   │ │ Milli   │ │ Custom │ │
│  │ (fast)  │ │ (code)  │ │ (vision)│ │ (heavy) │ │ (tool) │ │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘ └────────┘ │
└─────────────────────────────────────────────────────────────┘
          │                │                    │
          ▼                ▼                    ▼
┌─────────────────────────────────────────────────────────────┐
│                    ARTIFACT STORE                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Event Log   │  │  Findings   │  │   File System       │  │
│  │ (SQLite)    │  │  (JSONL)    │  │   (workspace)       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## Key Principles

### 1. Never Block

**Chatbot Pattern (Wrong):**
```
User: "Analyze this codebase"
AI: [works for 6 minutes]
User: [stares at spinner, can't do anything]
AI: "Here's the analysis"
```

**Automatic Computer Pattern (Right):**
```
User: "Analyze this codebase"
System: [spawns worker, returns immediately]
User: [continues working, spawns more workers]
System: [streams findings to dashboard in real-time]
User: "Great, now check for security issues"
System: [spawns second worker, both run in parallel]
```

### 2. Continuous Observability

Every worker emits events:
- `worker.spawned` - Worker started
- `worker.progress` - Intermediate findings
- `worker.complete` - Worker finished
- `worker.failed` - Worker crashed

Users observe via dashboard, not chat window.

### 3. Artifact Persistence

Workers write to the filesystem:
- `logs/actorcode/<worker_id>.jsonl` - Structured events
- `docs/research/<topic>.md` - Research outputs
- `findings/<category>.jsonl` - Categorized findings

No data lost when session ends.

### 4. Double-Texting Enabled

Users can send multiple inputs without waiting:
- Each input is an event
- System processes asynchronously
- No "please wait for me to finish" blocking

---

## Component Details

### Prompt Bar

The producer interface. Like a shell:
- Type command → press enter → command runs in background
- Output goes to dashboard, not chat window
- History, autocomplete, fuzzy search

### App Supervisors

Process managers for different domains:
- **Desktop Supervisor**: Manages window state, icons, focus
- **Chat Supervisor**: Manages conversation sessions
- **Research Supervisor**: Manages background research workers

Each supervisor:
- Spawns workers
- Monitors lifecycle
- Routes events
- Handles failures

### Event Bus

Pub/sub system for loose coupling:
```rust
// Workers publish
event_bus.publish("findings.new", Finding { ... });

// Dashboard subscribes
event_bus.subscribe("findings.new", |finding| {
    dashboard.add_finding(finding);
});
```

### Workers

Leaf nodes that do actual work:
- **Pico**: Fast, text-only (glm-4.7-flash)
- **Nano**: Coding-capable (glm-4.7)
- **Micro**: Multimodal (kimi-for-coding/k2p5)
- **Milli**: Heavy lifting (gpt-5.2-codex)

Workers are ephemeral. They:
1. Spawn
2. Do work
3. Write artifacts
4. Emit completion event
5. Exit

### Dashboard

Observability interface:
- **Network View**: See all running workers
- **Timeline View**: When workers started/stopped
- **Findings View**: Categorized research output
- **Logs View**: Real-time streaming logs

---

## Contrast with OpenAI Data Agent

| Feature | OpenAI Data Agent | ChoirOS Automatic Computer |
|---------|-------------------|---------------------------|
| **UX** | "Worked for 6m 1s" → final answer | Continuous streaming output |
| **Blocking** | User waits, can't interact | User continues, spawns more workers |
| **Visibility** | Black box during processing | Full observability via dashboard |
| **Artifacts** | Single answer delivered | Multiple artifacts accumulate |
| **Input** | One prompt → one response | Multiple prompts, parallel execution |
| **State** | Session-only | Persistent in filesystem |

---

## Implementation Phases

### Phase 1: Event Bus (Current)
- Add pub/sub to EventStoreActor
- Workers emit events
- Dashboard consumes events

### Phase 2: Supervisor Layer
- Desktop supervisor manages windows
- Chat supervisor manages sessions
- Research supervisor spawns background workers

### Phase 3: Prompt Bar
- Shell-like interface
- Command history
- Fuzzy autocomplete

### Phase 4: Artifact System
- Structured logging (JSONL)
- Automatic categorization
- Search and retrieval

### Phase 5: Full Integration
- All components wired together
- End-to-end automatic computer
- Producer/supervisor/worker flow

---

## Open Questions

1. **Event persistence**: Keep all events forever or rotate?
2. **Worker limits**: Max concurrent workers per user?
3. **Security**: How to sandbox workers from each other?
4. **Cost**: How to bill for background worker compute?

---

## References

- [Position Paper](/Users/wiz/choiros-settings-provider/docs/automatic_computer_position_paper.md)
- [Why Agents Need Actors](dev-blog/2026-02-01-why-agents-need-actors.md)
- [Actorcode Skill](../skills/actorcode/SKILL.md)

---

*The automatic computer is not a chatbot you talk to. It is infrastructure that works while you don't.*
