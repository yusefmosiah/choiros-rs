# ChoirOS Logging + Watcher Architecture Design

**Date:** 2026-02-08  
**Purpose:** Deep design for first-class Logging Actor + Watcher App with observability, evals, security monitoring, and replay/debugging  
**Status:** Design Document Ready for Implementation  

---

## Narrative Summary (1-minute read)

ChoirOS requires enterprise-grade logging architecture to support high-concurrency agent runs, multi-agent causality tracking, model-agnostic eval visibility, and UI visualization of live + historical logs. This design proposes a **hybrid dual-interface logging model** that cleanly separates **uActor → Actor** (secure prompt-envelope meta-coordination) from **AppActor → ToolActor** (typed tool contracts) while sharing a unified event envelope.

**Core architecture:**
- **Event Envelope:** W3C TraceContext-inspired with `trace_id`, `span_id`, `parent_span_id`, `causality_id`, plus ChoirOS-specific fields (`interface_kind`, `model`, `provider`, `security_labels`, `integrity_hash`)
- **Dual Interface Logging:** Separate event subtypes and payload contracts for uActor (meta-delegation) and AppActor (typed-tool) flows, with unified envelope for cross-flow querying
- **Storage:** Start with SQLite enhanced with WAL mode and connection pooling (10K events/sec target), evolve to hybrid hot/cold storage (SQLite for 30 days hot, JSONL archives for cold)
- **Watcher Actor:** Deterministic rule-based evaluation with 15 built-in rules, feedback loop prevention via event depth tracking, dedup windows, and circuit breakers
- **BAML Instrumentation:** Wrap 8 BAML lifecycle hooks with non-blocking event emission, capturing model metadata (latency, tokens, cost), tool representations (normalized vs raw), and error taxonomy
- **Security:** PII/secrets detection via regex patterns, redaction before storage, RBAC with 4 roles (User/Auditor/Admin/System), hash-chain integrity for tamper evidence
- **UI Evolution:** Phase 1 live stream console → Phase 2 filterable trace explorer → Phase 3 dashboard with concept-map visualization (force-directed graph + word-cloud sidebar)

---

## Table of Contents

1. [ChoirOS-Tailored Architecture Summary](#1-choiros-tailored-architecture-summary)
2. [Canonical Event Envelope](#2-canonical-event-envelope)
3. [Dual Interface Logging Model](#3-dual-interface-logging-model)
4. [BAML-Aware Instrumentation Plan](#4-baml-aware-instrumentation-plan)
5. [JSONL Schema + Examples](#5-jsonl-schema--examples)
6. [Storage/Indexing/Retention](#6-storageindexingretention)
7. [Watcher Actor Design](#7-watcher-actor-design)
8. [Logs App Evolution Roadmap](#8-logs-app-evolution-roadmap)
9. [Concept-Map / Word-Cloud UX Research](#9-concept-map--word-cloud-ux-research)
10. [Implementation Checklist](#10-implementation-checklist)
11. [Open Questions + Decisions Needed](#11-open-questions--decisions-needed)

---

## 1. ChoirOS-Tailored Architecture Summary

### 1.1 Dual Interaction Classes

ChoirOS has two fundamentally different interaction patterns that must be logged differently:

**Class 1: uActor → Actor (Meta-Coordination)**
- **Purpose:** Universal/meta actors coordinating other actors
- **Pattern:** Secure prompt-envelope style delegation
- **Examples:** ApplicationSupervisor → SessionSupervisor → ChatSupervisor, Supervisor spawning parallel workers
- **Interface:** Secure envelope with trace context, constraints, delegation intent
- **Logging Focus:** Causality chains, supervisor decisions, worker lifecycle, timeout/failure escalation

**Class 2: AppActor → ToolActor (Typed Tool Contracts)**
- **Purpose:** App-facing typed tool execution
- **Examples:** ChatAgent → TerminalActor (bash tool), ChatAgent → FileToolActor (read/write)
- **Pattern:** Structured type-safe tool invocations with validation
- **Interface:** Typed tool calls with explicit contracts (version, schema)
- **Logging Focus:** Tool args (normalized), execution results, performance metrics, errors

### 1.2 Why Separate But Unified

**Separate Payload Contracts:**
- uActor payloads: delegation metadata, constraints, supervision decisions
- AppActor payloads: tool names, args, results with type schemas
- Different semantics require different analysis and visualization

**Unified Event Envelope:**
- Both classes share the same top-level envelope fields
- Enables cross-flow queries (e.g., "show all events for trace X regardless of interface type")
- Unified causality tracking (trace_id, span_id, parent_span_id)
- Single EventStore schema with optional typed sub-payloads

**Visual Representation:**
```
┌─────────────────────────────────────────────────────────────────┐
│                    Unified Event Envelope                    │
├─────────────────────────────────────────────────────────────────┤
│ trace_id | span_id | parent_span_id | event_type          │
│ timestamp | actor_id | interface_kind | security_labels    │
├─────────────────────────────────────────────────────────────────┤
│                       Payload (Union)                       │
│  ┌────────────────────┬──────────────────────────────────┐  │
│  │ uActor Payload     │ AppActor Payload                 │  │
│  │ - delegation_id   │ - tool_name                   │  │
│  │ - constraints      │ - normalized_args              │  │
│  │ - supervision_dec │ - validation_result             │  │
│  │ - escalation_path │ - execution_result             │  │
│  └────────────────────┴──────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### 1.3 Why This Architecture

1. **Clean Separation of Concerns:**
   - uActor logic (supervision, delegation) isolated from AppActor logic (tool execution)
   - Enables different analysis tools: supervision trace explorer vs tool execution profiler

2. **Unified Causality:**
   - Single trace_id spans across uActor and AppActor flows
   - Parent/child relationships work across interface boundaries
   - Enables end-to-end tracing from supervisor decision to tool execution

3. **Typed Safety for Tools:**
   - AppActor → ToolActor uses explicit type contracts
   - Enables static validation, schema evolution, backward compatibility
   - Reduces runtime errors from malformed tool calls

4. **Secure by Design:**
   - uActor envelopes carry encryption signatures, policy references
   - Sensitive metadata (user prompts, secrets) redacted per interface type
   - RBAC enforced at EventStore query level

5. **Practical Implementation:**
   - Leverages existing EventStoreActor foundation
   - Extends current event schema with new fields (non-breaking)
   - Uses existing ractor patterns for actor communication

---

## 2. Canonical Event Envelope

### 2.1 Required Top-Level Fields

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct Event {
    // === Core Identification ===
    pub seq: i64,                              // Global logical sequence (EventStore assigned)
    pub event_id: String,                       // ULID for uniqueness
    pub timestamp: DateTime<Utc>,                // Wall-clock time
    pub event_type: String,                      // Hierarchical (e.g., "uactor.delegation")
    pub schema_version: u32,                     // Schema version at creation
    
    // === Actor Context ===
    pub actor_id: ActorId,                       // Source actor
    pub actor_type: Option<String>,               // "uactor", "appactor", "toolactor"
    pub session_id: Option<String>,               // Scope isolation
    pub thread_id: Option<String>,                // Scope isolation
    pub user_id: String,                        // User principal
    
    // === Distributed Tracing (W3C TraceContext-inspired) ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,                 // Root operation ID (16-byte hex)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,                  // Current span (8-byte hex)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,           // Direct causality
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,            // Business-level correlation (UUID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub causality_id: Option<String>,            // Async boundary continuity (UUID)
    
    // === Classification ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interface_kind: Option<InterfaceKind>,      // uactor_actor | appactor_toolactor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_kind: Option<EventKind>,             // ActorLifecycle, ModelInvocation, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<EventStatus>,               // Started, Completed, Failed, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,                 // For completed operations
    
    // === BAML/LLM Metadata ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,                    // Resolved model (e.g., "ClaudeBedrockOpus45")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,                 // Provider (e.g., "aws-bedrock")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baml_function: Option<String>,             // BAML function name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_refs: Option<Vec<String>>,          // Referenced policies
    
    // === Security ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security_labels: Option<Vec<SecurityLabel>>, // PII, secrets, sensitive
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redaction_state: Option<RedactionState>, // Which fields were redacted
    
    // === Integrity ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity_hash: Option<String>,             // Hash for tamper evidence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,             // Previous event's hash (chain)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrity_salt: Option<String>,            // Salt for hash chain
    
    // === Payload (Union) ===
    #[ts(type = "unknown")]
    pub payload: serde_json::Value,              // Event-specific data
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum InterfaceKind {
    UactorActor,
    AppactorToolactor,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum EventKind {
    ActorLifecycle,
    ModelInvocation,
    ToolExecution,
    ActorMessage,
    PolicyDecision,
    SystemEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum EventStatus {
    Started,
    InProgress,
    Completed,
    Failed { error: String },
    Cancelled,
    Retry { attempt: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub enum SecurityLabel {
    PiiEmail,
    PiiPhone,
    PiiSsn,
    PiiCreditCard,
    SecretApiKey,
    SecretPassword,
    SecretToken,
    SensitiveCommand,
    SensitiveFilePath,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../sandbox-ui/src/types/generated.ts")]
pub struct RedactionState {
    pub redacted_fields: Vec<String>,
    pub redaction_method: String,    // "regex", "field", "ml"
    pub original_hash: Option<String>, // For verification
}
```

### 2.2 ID Semantics

| ID | Purpose | Generation | Format | Lifetime |
|----|---------|------------|--------|----------|
| `event_id` | Deduplication & lookup | Per event (ULID) | 26-char ULID | Forever |
| `seq` | Global ordering | EventStore (auto-increment) | `i64` monotonic | Forever |
| `trace_id` | Root operation across all actors | At request entry | W3C 16-byte hex (32 chars) | Request lifecycle |
| `span_id` | Unique span within trace | Per event | W3C 8-byte hex (16 chars) | Single operation |
| `parent_span_id` | Direct causality | Inherited from parent | Same as span_id | Immutable |
| `correlation_id` | Business request tracking | User request | UUID/ULID | End-user session |
| `causality_id` | Async operation continuity | At async spawn | UUID | Cross-boundary ops |

### 2.3 Versioning Strategy

**Field:** `schema_version: u32` (required)

**Rules:**
1. Increment schema_version only on **breaking changes**
2. **Additive changes** (new optional fields) do not require version increment
3. **Renaming/removing fields** requires version increment + migration
4. Consumers must handle all versions ≤ current version
5. Producers must use latest schema_version

**Compatibility Matrix:**
| Schema Version | Changes | Backward Compatible | Forward Compatible |
|---------------|----------|---------------------|--------------------|
| 1 | Initial schema (current ChoirOS) | N/A | N/A |
| 2 | Add `trace_id`, `span_id` (optional) | Yes (old code ignores) | No (new data missing) |
| 3 | Make `trace_id`, `span_id` required | No (old events missing) | Yes (if backfilled) |
| 4 | Add `interface_kind` enum | Yes (old code ignores) | No (old events missing) |
| 5 | Add `integrity_hash`, `previous_hash` | Yes (old code ignores) | No (old events missing) |

**Migration Strategy:**
```sql
-- Phase 1: Add new optional fields (no version increment)
ALTER TABLE events ADD COLUMN trace_id TEXT;
ALTER TABLE events ADD COLUMN span_id TEXT;
ALTER TABLE events ADD COLUMN parent_span_id TEXT;
ALTER TABLE events ADD COLUMN interface_kind TEXT;

-- Phase 2: Make fields required (version increment)
ALTER TABLE events ADD COLUMN schema_version INTEGER DEFAULT 2;
-- Backfill existing events with default values
UPDATE events SET trace_id = '00000000000000000000000000000000000' WHERE trace_id IS NULL;

-- Phase 3: Add integrity fields (version increment)
ALTER TABLE events ADD COLUMN integrity_hash TEXT;
ALTER TABLE events ADD COLUMN previous_hash TEXT;
ALTER TABLE events ADD COLUMN integrity_salt TEXT;
```

---

## 3. Dual Interface Logging Model

### 3.1 Interface Kind Discriminator

**`interface_kind: InterfaceKind`** field distinguishes event flow types:

```rust
pub enum InterfaceKind {
    // uActor → Actor: Meta-coordination flows
    UactorActor,
    
    // AppActor → ToolActor: Typed tool execution flows
    AppactorToolactor,
}
```

**Usage:**
```rust
// uActor spawning SessionSupervisor
let event = Event {
    interface_kind: Some(InterfaceKind::UactorActor),
    event_type: "uactor.supervisor_spawned".to_string(),
    // ...
};

// ChatAgent calling bash tool
let event = Event {
    interface_kind: Some(InterfaceKind::AppactorToolactor),
    event_type: "appactor.tool_call_start".to_string(),
    // ...
};
```

### 3.2 uActor → Actor Subtype Schemas

**Purpose:** Log meta-coordination, delegation, supervision decisions

**Event Types:**
```rust
// uActor lifecycle
pub const EVENT_UACTOR_SPAWNED: &str = "uactor.spawned";
pub const EVENT_UACTOR_STOPPED: &str = "uactor.stopped";

// Delegation (secure envelope)
pub const EVENT_UACTOR_DELEGATION: &str = "uactor.delegation";
pub const EVENT_UACTOR_DELEGATION_ACCEPTED: &str = "uactor.delegation.accepted";
pub const EVENT_UACTOR_DELEGATION_REJECTED: &str = "uactor.delegation.rejected";

// Supervision decisions
pub const EVENT_UACTOR_SUPERVISION_DECISION: &str = "uactor.supervision_decision";
pub const EVENT_UACTOR_WORKER_SPAWNED: &str = "uactor.worker_spawned";
pub const EVENT_UACTOR_WORKER_TIMEOUT: &str = "uactor.worker_timeout";
pub const EVENT_UACTOR_WORKER_FAILURE: &str = "uactor.worker_failure";

// Escalation
pub const EVENT_UACTOR_ESCALATION: &str = "uactor.escalation";
```

**Payload Schema (Delegation):**
```json
{
  "delegation_id": "del-01HZ...",
  "from_actor": "ApplicationSupervisor",
  "to_actor": "SessionSupervisor",
  "secure_envelope": {
    "intent": "create_session",
    "constraints": {
      "max_duration_ms": 300000,
      "allowed_tools": ["chat", "terminal", "desktop"]
    },
    "signature": "sha256:abc123...",
    "policy_refs": ["policy_session_creation"]
  }
}
```

**Payload Schema (Supervision Decision):**
```json
{
  "decision_id": "dec-01HZ...",
  "supervisor": "SessionSupervisor",
  "decision_type": "spawn_worker",
  "reasoning": "User requested file analysis",
  "worker_config": {
    "worker_type": "Nano",
    "task": "analyze_files",
    "timeout_ms": 60000
  },
  "escalation_path": "timeout -> escalate_to_supervisor"
}
```

### 3.3 AppActor → ToolActor Subtype Schemas

**Purpose:** Log typed tool execution with explicit contracts

**Event Types:**
```rust
// Tool lifecycle
pub const EVENT_APPACTOR_TOOL_CALL_START: &str = "appactor.tool_call_start";
pub const EVENT_APPACTOR_TOOL_CALL_RESULT: &str = "appactor.tool_call_result";
pub const EVENT_APPACTOR_TOOL_CALL_ERROR: &str = "appactor.tool_call_error";

// Validation
pub const EVENT_APPACTOR_VALIDATION: &str = "appactor.validation";

// Tool-specific events
pub const EVENT_APPACTOR_BASH_COMMAND: &str = "appactor.bash_command";
pub const EVENT_APPACTOR_FILE_READ: &str = "appactor.file_read";
pub const EVENT_APPACTOR_FILE_WRITE: &str = "appactor.file_write";
```

**Payload Schema (Tool Call Start):**
```json
{
  "tool_call_id": "call-01HZ...",
  "tool_name": "bash",
  "contract": {
    "version": 1,
    "schema": {
      "type": "object",
      "properties": {
        "command": { "type": "string" },
        "cwd": { "type": "string" },
        "timeout_ms": { "type": "integer" }
      }
    }
  },
  "normalized_args": {
    "command": "ls -la",
    "cwd": "/tmp",
    "timeout_ms": 30000
  },
  "from_actor": "ChatAgent",
  "to_actor": "TerminalActor"
}
```

**Payload Schema (Tool Call Result):**
```json
{
  "tool_call_id": "call-01HZ...",
  "tool_name": "bash",
  "execution_result": {
    "status": "success",
    "exit_code": 0,
    "output": "total 24\ndrwxr-xr-x ...",
    "duration_ms": 1234,
    "redacted_output": "[REDACTED_FILE_PATHS]"
  },
  "validation_result": {
    "schema_version": 1,
    "validation_passed": true,
    "errors": []
  }
}
```

### 3.4 Cross-Interface Tracing Example

**Scenario:** ApplicationSupervisor delegates to SessionSupervisor, which spawns ChatAgent, which calls bash tool

```
Event Flow (unified trace):

1. [uActor] trace_id: T1, span_id: S1, parent: null
   event_type: "uactor.delegation"
   interface_kind: UactorActor
   payload: { to_actor: "SessionSupervisor", intent: "create_chat" }

2. [uActor] trace_id: T1, span_id: S2, parent: S1
   event_type: "uactor.delegation.accepted"
   interface_kind: UactorActor
   payload: { delegation_id: "del-123", decision: "accept" }

3. [uActor] trace_id: T1, span_id: S3, parent: S2
   event_type: "uactor.actor_spawned"
   interface_kind: UactorActor
   payload: { actor_id: "ChatActor", supervisor: "SessionSupervisor" }

4. [AppActor] trace_id: T1, span_id: S4, parent: S3
   event_type: "baml.model_invoke.start"
   interface_kind: AppactorToolactor
   payload: { model: "ClaudeBedrockOpus45", baml_function: "PlanAction" }

5. [AppActor] trace_id: T1, span_id: S5, parent: S4
   event_type: "appactor.tool_call_start"
   interface_kind: AppactorToolactor
   payload: { tool_name: "bash", normalized_args: { command: "ls -la" } }

6. [AppActor] trace_id: T1, span_id: S6, parent: S5
   event_type: "appactor.tool_call_result"
   interface_kind: AppactorToolactor
   payload: { tool_call_id: "call-456", execution_result: { status: "success" } }
```

**Key Benefits:**
- Single `trace_id: T1` spans entire flow
- Clear parent/child relationships across interface boundaries
- Watcher can query `WHERE trace_id = 'T1'` to see end-to-end flow
- UI can filter by `interface_kind` to show only uActor or only AppActor flows

### 3.5 Watcher Consumption Model

Watcher actors consume both interface types through unified query patterns:

```rust
// Watcher subscribes to all events
let subscription = WatcherSubscription {
    topic: "*".to_string(),  // All events
    actor_id: None,
    payload_filters: vec![],
};

// Watcher can filter by interface_kind
let rule_uactor = WatchRule {
    name: "Monitor uActor Delegations".to_string(),
    ast: RuleAst::FieldCondition {
        path: "$.interface_kind".to_string(),
        operator: ComparisonOperator::Equal,
        value: serde_json::json!("uactor_actor"),
    },
    // ...
};

let rule_tool = WatchRule {
    name: "Monitor Tool Executions".to_string(),
    ast: RuleAst::FieldCondition {
        path: "$.interface_kind".to_string(),
        operator: ComparisonOperator::Equal,
        value: serde_json::json!("appactor_toolactor"),
    },
    // ...
};

// Watcher can filter by event type across interfaces
let rule_all_model_calls = WatchRule {
    name: "Slow Model Calls".to_string(),
    ast: RuleAst::FieldCondition {
        path: "$.event_type".to_string(),
        operator: ComparisonOperator::Contains,
        value: serde_json::json!("model_invoke"),
    },
    // ...
};
```

---

## 4. BAML-Aware Instrumentation Plan

### 4.1 Instrumentation Hook List

Wrap these exact functions in `sandbox/src/baml_client/functions/async_client.rs` and `sandbox/src/actors/chat_agent.rs`:

| Hook Location | Function | Purpose | Event Type |
|--------------|----------|---------|------------|
| `chat_agent.rs:676` | `PlanAction::call()` | Plan LLM call (determines tools) | `baml.call_started` |
| `chat_agent.rs:676` | `PlanAction::call()` (result) | Plan completion | `baml.call_completed` / `baml.call_failed` |
| `chat_agent.rs:646` | `ModelRegistry::resolve()` | Model selection | `baml.model_resolution` |
| `chat_agent.rs:706` | Tool extraction loop | Parse tool calls from response | `baml.tool_extracted` |
| `chat_agent.rs:794` | `SynthesizeResponse::call()` | Synthesis call (final response) | `baml.synthesis_started` |
| `chat_agent.rs:794` | `SynthesizeResponse::call()` (result) | Synthesis completion | `baml.synthesis_completed` / `baml.synthesis_failed` |
| `model_config.rs:100` | `ModelRegistry::create_runtime_client_registry_for_model()` | Client registry creation | `baml.client_registry_created` |

**Implementation Strategy:** Wrap calls in `ChatAgent::handle_process_message()` without modifying generated BAML code:

```rust
// In chat_agent.rs, around line 676-681
let plan_start = std::time::Instant::now();
let call_id = ulid::Ulid::new().to_string();

// Emit BamlCallStarted event (non-blocking)
emit_baml_event_non_blocking(
    &state.event_store,
    state.args.actor_id.clone(),
    state.args.user_id.clone(),
    "baml.call_started",
    json!({
        "call_id": call_id,
        "function": "PlanAction",
        "model": model_used,
        "model_source": model_source,
        "session_id": session_id,
        "thread_id": thread_id,
    }),
).await;

let plan_result = crate::baml_client::B
    .PlanAction
    .with_client_registry(&client_registry)
    .call(&state.messages, &system_context, &tools_description)
    .await;

let latency_ms = plan_start.elapsed().as_millis() as i64;
match &plan_result {
    Ok(plan) => {
        emit_baml_event_non_blocking(
            &state.event_store,
            state.args.actor_id.clone(),
            state.args.user_id.clone(),
            "baml.call_completed",
            json!({
                "call_id": call_id,
                "function": "PlanAction",
                "model": model_used,
                "latency_ms": latency_ms,
                "tool_calls_count": plan.tool_calls.len(),
            }),
        ).await;
        
        // Extract and log tools
        for tool_call in &plan.tool_calls {
            emit_baml_event_non_blocking(
                &state.event_store,
                state.args.actor_id.clone(),
                state.args.user_id.clone(),
                "baml.tool_extracted",
                json!({
                    "parent_call_id": call_id,
                    "tool_name": tool_call.tool_name,
                    "normalized_args": redact_tool_args(&tool_call.args),
                    "raw_model_output": plan.thinking,
                    "parse_confidence": plan.confidence,
                    "reasoning": tool_call.reasoning,
                    "session_id": session_id,
                    "thread_id": thread_id,
                }),
            ).await;
        }
    }
    Err(e) => {
        emit_baml_event_non_blocking(
            &state.event_store,
            state.args.actor_id.clone(),
            state.args.user_id.clone(),
            "baml.call_failed",
            json!({
                "call_id": call_id,
                "function": "PlanAction",
                "model": model_used,
                "latency_ms": latency_ms,
                "error_type": classify_baml_error(e),
                "error_message": redact_error_message(e),
                "retry_attempt": retry_count,
                "session_id": session_id,
                "thread_id": thread_id,
            }),
        ).await;
    }
}
```

### 4.2 Event Schemas for BAML Lifecycle

**`baml.call_started`**
```json
{
  "call_id": "01HZ...",
  "function": "PlanAction" | "SynthesizeResponse" | "QuickResponse",
  "model": "ClaudeBedrockOpus45",
  "model_source": "request" | "app" | "user" | "env_default" | "fallback",
  "provider": "aws-bedrock" | "anthropic" | "openai-generic",
  "correlation_id": "optional_parent_call_id",
  "timestamp_ms": 1739001234567
}
```

**`baml.call_completed`**
```json
{
  "call_id": "01HZ...",
  "function": "PlanAction",
  "model": "ClaudeBedrockOpus45",
  "latency_ms": 1234,
  "tokens_used": {
    "prompt_tokens": 1200,
    "completion_tokens": 800,
    "total_tokens": 2000
  },
  "cost_estimate_usd": 0.012,
  "stream_chunks": 42
}
```

**`baml.model_resolution`**
```json
{
  "call_id": "01HZ...",
  "requested_model": "ClaudeBedrock",
  "resolved_model": "ClaudeBedrockOpus45",
  "resolution_source": "request" | "app" | "user" | "env_default" | "fallback",
  "provider": "aws-bedrock",
  "provider_config": {
    "region": "us-east-1",
    "base_url": "[REDACTED]",
    "api_key_env": "ZAI_API_KEY"
  },
  "client_aliases": ["ClaudeBedrock", "GLM47"]
}
```

**`baml.tool_extracted`**
```json
{
  "call_id": "01HZ...",
  "parent_call_id": "01HZ...",
  "tool_name": "bash" | "read_file" | "write_file",
  "normalized_args": {
    "command": "ls -la",
    "cwd": "/tmp"
  },
  "raw_model_output": "Thinking process...",
  "parse_confidence": 0.95,
  "reasoning": "Need to list directory contents"
}
```

**`baml.call_failed`**
```json
{
  "call_id": "01HZ...",
  "function": "PlanAction",
  "model": "ClaudeBedrockOpus45",
  "latency_ms": 5678,
  "error_type": "connection_error" | "parse_error" | "timeout" | "rate_limit" | "content_filter" | "auth_error" | "unknown",
  "error_code": "bedrock_500" | "anthropic_429",
  "error_message": "[REDACTED]",
  "retry_attempt": 2,
  "max_retries": 2
}
```

### 4.3 Model Metadata Capture

**Token Pricing Table (add to `model_config.rs`):**
```rust
#[derive(Debug, Clone)]
pub struct TokenPricing {
    pub prompt_per_1k: f64,
    pub completion_per_1k: f64,
}

pub fn get_token_pricing(model_id: &str) -> Option<TokenPricing> {
    match model_id {
        "ClaudeBedrockOpus45" | "ClaudeBedrockOpus46" => Some(TokenPricing {
            prompt_per_1k: 15.0,
            completion_per_1k: 75.0,
        }),
        "ClaudeBedrockSonnet45" => Some(TokenPricing {
            prompt_per_1k: 3.0,
            completion_per_1k: 15.0,
        }),
        "ClaudeBedrockHaiku45" => Some(TokenPricing {
            prompt_per_1k: 0.25,
            completion_per_1k: 1.25,
        }),
        "ZaiGLM47" => Some(TokenPricing {
            prompt_per_1k: 5.0,
            completion_per_1k: 15.0,
        }),
        "ZaiGLM47Flash" => Some(TokenPricing {
            prompt_per_1k: 0.1,
            completion_per_1k: 0.4,
        }),
        _ => None,
    }
}

pub fn estimate_cost_usd(model_id: &str, prompt_tokens: i64, completion_tokens: i64) -> Option<f64> {
    let pricing = get_token_pricing(model_id)?;
    let prompt_cost = (prompt_tokens as f64 / 1000.0) * pricing.prompt_per_1k;
    let completion_cost = (completion_tokens as f64 / 1000.0) * pricing.completion_per_1k;
    Some(prompt_cost + completion_cost)
}
```

### 4.4 Non-Blocking Event Emission

**Fire-and-forget pattern** to avoid blocking LLM calls:

```rust
use ractor::cast;

async fn emit_baml_event_non_blocking(
    event_store: &ActorRef<EventStoreMsg>,
    actor_id: String,
    user_id: String,
    event_type: &str,
    payload: serde_json::Value,
) {
    let event = AppendEvent {
        event_type: event_type.to_string(),
        payload,
        actor_id,
        user_id,
    };

    let store_ref = event_store.clone();
    tokio::spawn(async move {
        match ractor::call!(store_ref, |reply| EventStoreMsg::Append {
            event,
            reply,
        }) {
            Ok(Err(e)) => tracing::warn!("BAML event persistence failed: {}", e),
            Err(e) => tracing::warn!("BAML event send failed: {}", e),
            _ => {}
        }
    });
}
```

---

## 5. JSONL Schema + Examples

### 5.1 JSONL Format

Each line is a JSON object representing one event:

```
{"seq":1001,"event_id":"01HZ...","timestamp":"2026-02-08T12:34:56.789Z","event_type":"uactor.delegation","schema_version":1,"actor_id":"ApplicationSupervisor","actor_type":"uactor","trace_id":"4bf92f3577b34da6a3ce929d0e0e4736","span_id":"00f067aa0ba902b","parent_span_id":null,"correlation_id":"corr-123","causality_id":null,"interface_kind":"uactor_actor","event_kind":"ActorMessage","status":"Completed","duration_ms":5,"model":null,"provider":null,"baml_function":null,"policy_refs":["policy_supervision"],"security_labels":[],"redaction_state":null,"integrity_hash":"sha256:abc...","previous_hash":null,"integrity_salt":"salt-456","payload":{"delegation_id":"del-01HZ...","from_actor":"ApplicationSupervisor","to_actor":"SessionSupervisor","secure_envelope":{"intent":"create_session","constraints":{"max_duration_ms":300000},"signature":"sha256:def..."}},"user_id":"user-1","session_id":"session-123","thread_id":null}
```

### 5.2 Example JSONL Lines

**Agent Run Start/End:**
```json
{"seq":1001,"event_id":"01HZABCD1234567890123456","timestamp":"2026-02-08T12:34:56.789Z","event_type":"uactor.worker_spawned","schema_version":1,"actor_id":"SessionSupervisor","actor_type":"uactor","trace_id":"4bf92f3577b34da6a3ce929d0e0e4736","span_id":"00f067aa0ba902b","parent_span_id":null,"correlation_id":"session-123","causality_id":"causal-abc","interface_kind":"uactor_actor","event_kind":"ActorLifecycle","status":"Started","duration_ms":null,"model":null,"provider":null,"baml_function":null,"policy_refs":["policy_worker_spawn"],"security_labels":[],"redaction_state":null,"integrity_hash":"sha256:abc123","previous_hash":null,"integrity_salt":"salt-789","payload":{"worker_id":"worker-nano-456","worker_type":"Nano","task":"analyze_files","timeout_ms":60000},"user_id":"user-1","session_id":"session-123","thread_id":"thread-456"}
{"seq":1050,"event_id":"01HZEFGH1234567890123457","timestamp":"2026-02-08T12:35:58.123Z","event_type":"uactor.worker_completed","schema_version":1,"actor_id":"worker-nano-456","actor_type":"worker","trace_id":"4bf92f3577b34da6a3ce929d0e0e4736","span_id":"00f067aa0ba902b","parent_span_id":"00f067aa0ba902c","correlation_id":"session-123","causality_id":"causal-abc","interface_kind":"appactor_toolactor","event_kind":"ToolExecution","status":"Completed","duration_ms":61434,"model":null,"provider":null,"baml_function":null,"policy_refs":[],"security_labels":[],"redaction_state":null,"integrity_hash":"sha256:def456","previous_hash":"sha256:abc123","integrity_salt":"salt-789","payload":{"worker_id":"worker-nano-456","result":{"status":"success","files_analyzed":42,"findings":[]},"user_id":"user-1","session_id":"session-123","thread_id":"thread-456"}
```

**BAML Request/Response:**
```json
{"seq":1100,"event_id":"01HZIJKL1234567890123458","timestamp":"2026-02-08T12:36:00.456Z","event_type":"baml.call_started","schema_version":1,"actor_id":"ChatActor","actor_type":"appactor","trace_id":"004067aa0ba902b766872651a637492","span_id":"00f067aa0ba902b","parent_span_id":"00f067aa0ba902a","correlation_id":"session-123","causality_id":null,"interface_kind":"appactor_toolactor","event_kind":"ModelInvocation","status":"Started","duration_ms":null,"model":"ClaudeBedrockOpus45","provider":"aws-bedrock","baml_function":"PlanAction","policy_refs":[],"security_labels":[],"redaction_state":null,"integrity_hash":"sha256:ghi789","previous_hash":"sha256:def456","integrity_salt":"salt-012","payload":{"call_id":"baml-call-789","function":"PlanAction","model":"ClaudeBedrockOpus45","model_source":"request","timestamp_ms":1739000960456},"user_id":"user-1","session_id":"session-123","thread_id":"thread-456"}
{"seq":1200,"event_id":"01HZMNOP1234567890123459","timestamp":"2026-02-08T12:36:01.690Z","event_type":"baml.call_completed","schema_version":1,"actor_id":"ChatActor","actor_type":"appactor","trace_id":"004067aa0ba902b766872651a637492","span_id":"00f067aa0ba902b","parent_span_id":"00f067aa0ba902a","correlation_id":"session-123","causality_id":null,"interface_kind":"appactor_toolactor","event_kind":"ModelInvocation","status":"Completed","duration_ms":1234,"model":"ClaudeBedrockOpus45","provider":"aws-bedrock","baml_function":"PlanAction","policy_refs":[],"security_labels":[],"redaction_state":null,"integrity_hash":"sha256:jkl012","previous_hash":"sha256:ghi789","integrity_salt":"salt-012","payload":{"call_id":"baml-call-789","function":"PlanAction","model":"ClaudeBedrockOpus45","latency_ms":1234,"tokens_used":{"prompt_tokens":1200,"completion_tokens":800,"total_tokens":2000},"cost_estimate_usd":0.012,"stream_chunks":0},"user_id":"user-1","session_id":"session-123","thread_id":"thread-456"}
```

**Tool Call/Result:**
```json
{"seq":1250,"event_id":"01HZQRST1234567890123460","timestamp":"2026-02-08T12:36:02.100Z","event_type":"appactor.tool_call_start","schema_version":1,"actor_id":"ChatActor","actor_type":"appactor","trace_id":"004067aa0ba902b766872651a637492","span_id":"00f067aa0ba902d","parent_span_id":"00f067aa0ba902c","correlation_id":"session-123","causality_id":null,"interface_kind":"appactor_toolactor","event_kind":"ToolExecution","status":"Started","duration_ms":null,"model":null,"provider":null,"baml_function":null,"policy_refs":["policy_bash_tool"],"security_labels":["SensitiveCommand"],"redaction_state":{"redacted_fields":["command"],"redaction_method":"regex","original_hash":"sha256:uvw345"},"integrity_hash":"sha256:mno345","previous_hash":"sha256:jkl012","integrity_salt":"salt-345","payload":{"tool_call_id":"call-456","tool_name":"bash","contract":{"version":1},"normalized_args":{"command":"[REDACTED]","cwd":"/tmp","timeout_ms":30000},"from_actor":"ChatActor","to_actor":"TerminalActor"},"user_id":"user-1","session_id":"session-123","thread_id":"thread-456"}
{"seq":1300,"event_id":"01HZUVWX1234567890123461","timestamp":"2026-02-08T12:36:03.450Z","event_type":"appactor.tool_call_result","schema_version":1,"actor_id":"TerminalActor","actor_type":"toolactor","trace_id":"004067aa0ba902b766872651a637492","span_id":"00f067aa0ba902e","parent_span_id":"00f067aa0ba902d","correlation_id":"session-123","causality_id":null,"interface_kind":"appactor_toolactor","event_kind":"ToolExecution","status":"Completed","duration_ms":3350,"model":null,"provider":null,"baml_function":null,"policy_refs":["policy_bash_tool"],"security_labels":[],"redaction_state":null,"integrity_hash":"sha256:pqr678","previous_hash":"sha256:mno345","integrity_salt":"salt-345","payload":{"tool_call_id":"call-456","tool_name":"bash","execution_result":{"status":"success","exit_code":0,"output":"[REDACTED_FILE_PATHS]","duration_ms":3350},"validation_result":{"schema_version":1,"validation_passed":true,"errors":[]}},"user_id":"user-1","session_id":"session-123","thread_id":"thread-456"}
```

**uActor Delegation Message:**
```json
{"seq":1000,"event_id":"01HZ1234ABCD567890123455","timestamp":"2026-02-08T12:34:55.000Z","event_type":"uactor.delegation","schema_version":1,"actor_id":"ApplicationSupervisor","actor_type":"uactor","trace_id":"4bf92f3577b34da6a3ce929d0e0e4736","span_id":"00f067aa0ba902b","parent_span_id":null,"correlation_id":"session-123","causality_id":null,"interface_kind":"uactor_actor","event_kind":"ActorMessage","status":"Completed","duration_ms":2,"model":null,"provider":null,"baml_function":null,"policy_refs":["policy_delegation_secure"],"security_labels":["SecureEnvelope"],"redaction_state":{"redacted_fields":["signature"],"redaction_method":"field","original_hash":"sha256:abc456"},"integrity_hash":"sha256:def789","previous_hash":null,"integrity_salt":"salt-901","payload":{"delegation_id":"del-01HZ...","from_actor":"ApplicationSupervisor","to_actor":"SessionSupervisor","secure_envelope":{"intent":"create_session","constraints":{"max_duration_ms":300000,"allowed_tools":["chat","terminal","desktop"]},"signature":"[REDACTED]"}},"user_id":"user-1","session_id":"session-123","thread_id":null}
```

**Policy Decision:**
```json
{"seq":1002,"event_id":"01HZ5678CDEF9012345678","timestamp":"2026-02-08T12:34:55.500Z","event_type":"uactor.supervision_decision","schema_version":1,"actor_id":"ApplicationSupervisor","actor_type":"uactor","trace_id":"4bf92f3577b34da6a3ce929d0e0e4736","span_id":"00f067aa0ba902c","parent_span_id":"00f067aa0ba902b","correlation_id":"session-123","causality_id":null,"interface_kind":"uactor_actor","event_kind":"PolicyDecision","status":"Completed","duration_ms":10,"model":null,"provider":null,"baml_function":null,"policy_refs":["policy_supervision_decision"],"security_labels":[],"redaction_state":null,"integrity_hash":"sha256:ghi012","previous_hash":"sha256:def789","integrity_salt":"salt-902","payload":{"decision_id":"dec-01HZ...","supervisor":"SessionSupervisor","decision_type":"approve_delegation","reasoning":"User has permission to create session","policy":"policy_supervision_decision","action":"allow"},"user_id":"user-1","session_id":"session-123","thread_id":null}
```

**Watcher Alert:**
```json
{"seq":9999,"event_id":"01HZABCD901234567890123","timestamp":"2026-02-08T12:40:00.000Z","event_type":"alert.emitted","schema_version":1,"actor_id":"WatcherActor-security","actor_type":"watcher","trace_id":null,"span_id":null,"parent_span_id":null,"correlation_id":null,"causality_id":null,"interface_kind":null,"event_kind":"SystemEvent","status":"Completed","duration_ms":50,"model":null,"provider":null,"baml_function":null,"policy_refs":["policy_pii_detection"],"security_labels":["SecurityAlert"],"redaction_state":null,"integrity_hash":"sha256:jkl345","previous_hash":null,"integrity_salt":"salt-999","payload":{"alert_id":"alert-pii-001","rule_id":"rule-pii-email","severity":"high","category":"security","message":"PII detected: email address in chat message","trigger_event":{"event_id":"evt-789","event_type":"chat.user_msg"},"source":"watcher-security-1","is_watcher_generated":true},"user_id":"system","session_id":null,"thread_id":null}
```

**UI Interaction:**
```json
{"seq":10001,"event_id":"01HZ9012345678901234567","timestamp":"2026-02-08T12:45:30.000Z","event_type":"ui.concept_clicked","schema_version":1,"actor_id":"LogsApp","actor_type":"ui","trace_id":null,"span_id":null,"parent_span_id":null,"correlation_id":null,"causality_id":null,"interface_kind":null,"event_kind":"SystemEvent","status":"Completed","duration_ms":null,"model":null,"provider":null,"baml_function":null,"policy_refs":[],"security_labels":[],"redaction_state":null,"integrity_hash":"sha256:stu456","previous_hash":null,"integrity_salt":"salt-888","payload":{"concept_id":"actor:chat-456","concept_type":"Actor","click_position":{"x":120,"y":340},"action":"select","view_state":{"zoom_level":1.5,"filter":{"interface_kind":["appactor_toolactor"]}},"user_id":"user-1"},"user_id":"user-1","session_id":"session-123","thread_id":null}
```

### 5.3 Integrity Fields in JSONL

Each event includes:
- `integrity_hash`: SHA-256 of (seq + event_id + timestamp + event_type + payload + previous_hash + salt)
- `previous_hash`: `integrity_hash` of previous event (null for first event)
- `integrity_salt`: Random ULID used for hashing

**Tamper Verification:**
```rust
pub fn verify_integrity_chain(events: &[Event]) -> Result<(), IntegrityError> {
    let mut expected_prev_hash: Option<String> = None;
    
    for event in events {
        let actual_hash = compute_integrity_hash(
            event.seq,
            &event.event_id,
            &event.timestamp,
            &event.event_type,
            &event.payload,
            expected_prev_hash.as_deref(),
            &event.integrity_salt.as_ref().ok_or(IntegrityError::MissingSalt)?,
        )?;
        
        if actual_hash != event.integrity_hash.as_ref().ok_or(IntegrityError::MissingHash)? {
            return Err(IntegrityError::Tampered {
                seq: event.seq,
                event_id: event.event_id.clone(),
                expected_hash: actual_hash,
                actual_hash: event.integrity_hash.clone().unwrap_or_default(),
            });
        }
        
        expected_prev_hash = Some(actual_hash);
    }
    
    Ok(())
}
```

---

## 6. Storage/Indexing/Retention

### 6.1 Architecture Comparison

| Aspect | JSONL-Only | DB-Only (SQLite) | Hybrid (Recommended) |
|--------|-----------|------------------|---------------------|
| **Write Throughput** | Very High (append-only) | Medium (WAL: 5-10K/sec) | High (DB for hot, async to JSONL) |
| **Query Performance** | Slow (full-file scan) | Fast (indexed queries) | Fast for recent, slow for archived |
| **Real-time Streaming** | Hard (file watching) | Easy (DB triggers) | Easy (DB-based) |
| **Schema Evolution** | Flexible (no schema) | Requires migrations | Flexible (JSONL archival) |
| **Backup/Restore** | Simple (copy files) | Requires dump/restore | Two-tier (DB + file backup) |
| **Retention/Rotation** | Easy (delete files) | Requires DELETE + VACUUM | Easy (rotate JSONL) |
| **Best For** | High-volume logs, archival | Real-time queries, ACID | Mixed workload (hot + cold) |

**ChoirOS Recommendation:** Hybrid (SQLite for 30 days hot + JSONL archives for cold)

### 6.2 Recommended Architecture: Phase Evolution

**Phase 1: Enhanced SQLite (Immediate - Week 1)**
```sql
-- Enable WAL mode
PRAGMA journal_mode = WAL;

-- Increase cache size (default 2MB, recommend 64MB)
PRAGMA cache_size = -64000;

-- Optimize for append-heavy workload
PRAGMA synchronous = NORMAL;
```

**Performance Target:** 10,000 events/sec (from current 1,000/sec)

**Phase 2: Monthly Partitioning (Weeks 2-4)**
```sql
-- Monthly tables
CREATE TABLE events_2026_02 (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    actor_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'system',
    session_id TEXT,
    thread_id TEXT,
    -- Trace context fields
    trace_id TEXT,
    span_id TEXT,
    parent_span_id TEXT,
    correlation_id TEXT,
    causality_id TEXT,
    -- Classification
    interface_kind TEXT,
    event_kind TEXT,
    event_status TEXT,
    duration_ms INTEGER,
    -- BAML metadata
    model TEXT,
    provider TEXT,
    baml_function TEXT,
    policy_refs TEXT,
    -- Security
    security_labels TEXT,
    redaction_state TEXT,
    -- Integrity
    integrity_hash TEXT,
    previous_hash TEXT,
    integrity_salt TEXT,
    schema_version INTEGER DEFAULT 1
);

CREATE INDEX idx_events_2026_02_actor_session_thread_seq 
    ON events_2026_02(actor_id, session_id, thread_id, seq);
```

**Phase 3: Hybrid Hot/Cold Storage (Month 2)**
```
Hot Storage (SQLite, last 30 days):
  ├── events_2026_01 (recent)
  ├── events_2026_02 (current)
  └── events_2026_03 (next month)

Cold Storage (JSONL, archived daily):
  /data/events/archived/
    ├── 2026-01-01.jsonl.gz (compressed)
    ├── 2026-01-02.jsonl.gz
    └── ...
```

**Archiver Service:**
```rust
async fn archive_old_events() {
    let cutoff = Utc::now() - Duration::days(30);
    
    // Query events older than 30 days
    let events = query_events_older_than(cutoff).await;
    
    // Write to JSONL (compressed)
    write_jsonl(&events, "/data/events/archived").await;
    
    // Delete from hot storage (in batches)
    delete_archived_events(cutoff).await;
    
    // VACUUM to reclaim space
    vacuum_database().await;
}
```

### 6.3 Index Strategy

**Critical Indexes:**
```sql
-- Most common: scoped trace reconstruction
CREATE INDEX idx_trace_lookup 
    ON events(actor_id, session_id, thread_id, seq);

-- Time-series queries
CREATE INDEX idx_time_range 
    ON events(timestamp);

-- Event type filtering
CREATE INDEX idx_event_type_actor 
    ON events(event_type, actor_id);

-- Trace context queries
CREATE INDEX idx_trace_id 
    ON events(trace_id);

-- Composite for WebSocket streaming
CREATE INDEX idx_ws_stream 
    ON events(actor_id, session_id, thread_id, timestamp);

-- Interface kind filtering
CREATE INDEX idx_interface_kind 
    ON events(interface_kind, event_type);
```

### 6.4 Retention Policies

| Event Type | Hot Retention | Cold Retention | Reason |
|------------|--------------|---------------|---------|
| `uactor.*` | 90 days | 1 year | Supervisor coordination history |
| `appactor.tool_*` | 7 days (bash), 30 days (other) | 90 days | Bash logs sensitive, longer for other tools |
| `chat.user_msg` | 30 days | 1 year | User conversations |
| `baml.*` | 30 days | 1 year | LLM call history for evals |
| `alert.*` | 90 days | 1 year | Security monitoring history |
| `ui.*` | 7 days | 90 days | UI interaction analytics |

### 6.5 Query Patterns

**Get all events for a trace:**
```sql
SELECT seq, timestamp, event_type, payload 
FROM events 
WHERE trace_id = ?1 
  AND session_id = ?2 
  AND thread_id = ?3 
ORDER BY seq;
```

**Compare two runs:**
```sql
SELECT 
  seq, event_type, payload, 'run-1' as run_id
FROM events WHERE session_id = ?1
UNION ALL
SELECT 
  seq, event_type, payload, 'run-2' as run_id
FROM events WHERE session_id = ?2
ORDER BY seq, run_id;
```

**Get tool calls by interface kind:**
```sql
SELECT seq, timestamp, event_type, payload 
FROM events 
WHERE interface_kind = 'appactor_toolactor'
  AND event_type = 'appactor.tool_call_start'
ORDER BY seq DESC
LIMIT 100;
```

**Get uActor delegations:**
```sql
SELECT seq, timestamp, event_type, payload 
FROM events 
WHERE interface_kind = 'uactor_actor'
  AND event_type = 'uactor.delegation'
ORDER BY seq DESC;
```

---

## 7. Watcher Actor Design

### 7.1 Subscription Model

Three subscription types:

**1. Topic-based:** Subscribe to exact topic or wildcard pattern
```rust
let subscription = WatcherSubscription {
    topic: "chat.*".to_string(),  // Matches chat.user_msg, chat.assistant_msg
    actor_id: None,
    scope: None,
    payload_filters: vec![],
    max_events_per_sec: None,
};
```

**2. Pattern-based:** Filter events by payload structure/value using JSONPath
```rust
let subscription = WatcherSubscription {
    topic: "appactor.tool_call_start".to_string(),
    actor_id: None,
    scope: None,
    payload_filters: vec![
        PayloadFilter {
            path: "$.tool_name".to_string(),
            condition: FilterCondition::Equals {
                value: serde_json::json!("bash"),
            },
        },
        PayloadFilter {
            path: "$.normalized_args.command".to_string(),
            condition: FilterCondition::Contains {
                value: "rm -rf".to_string(),
            },
        },
    ],
    max_events_per_sec: Some(100),
};
```

**3. Actor-scoped:** Subscribe to events from specific actor IDs
```rust
let subscription = WatcherSubscription {
    topic: "*".to_string(),  // All topics
    actor_id: Some("ChatAgent".to_string()),
    scope: Some(ScopeFilter {
        session_id: Some("session-123".to_string()),
        thread_id: Some("thread-456".to_string()),
    }),
    payload_filters: vec![],
    max_events_per_sec: Some(1000),
};
```

### 7.2 Rule Engine

**Deterministic rule evaluation** (ML-based deferred to post-hardening)

**Rule AST:**
```rust
pub enum RuleAst {
    FieldCondition {
        path: String,  // JSONPath
        operator: ComparisonOperator,
        value: serde_json::Value,
    },
    And { conditions: Vec<RuleAst> },
    Or { conditions: Vec<RuleAst> },
    Not { condition: Box<RuleAst> },
    CountInWindow { window_ms: u64, threshold: usize },
    RegexMatch { path: String, pattern: String },
}
```

**15 Built-in Rules:**

**Security Rules (5):**
1. PII Email Detection: `RegexMatch { path: "$.payload.text", pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}" }`
2. PII Phone Detection: `RegexMatch { path: "$.payload.text", pattern: r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b" }`
3. PII Credit Card: `RegexMatch { path: "$.payload.text", pattern: r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b" }`
4. SQL Injection Pattern: `Or { conditions: [Contains { path: "$.payload.cmd", value: "' OR '1'='1" }, Contains { path: "$.payload.cmd", value: "'; DROP TABLE" } ] }`
5. Command Injection Pattern: `Contains { path: "$.payload.command", value: "&& rm -rf /" }`

**Policy Rules (5):**
6. Block Dangerous Commands: `Or { conditions: [Contains { path: "$.payload.normalized_args.command", value: "rm -rf /" }, Contains { path: "$.payload.normalized_args.command", value: "dd if=/dev/zero" } ] }`
7. Tool Usage Rate Limit: `CountInWindow { window_ms: 60000, threshold: 100 }`
8. File Access Restriction: `Contains { path: "$.payload.normalized_args.path", value: "/etc/passwd" }`
9. Network Access Restriction: `Contains { path: "$.payload.url", value: "192.168." }`
10. Session Timeout: `GreaterThan { path: "$.payload.idle_duration_ms", value: 1800000 }`

**Observability Rules (5):**
11. Slow Model Call: `GreaterThan { path: "$.duration_ms", value: 10000 }`
12. Worker Failure Rate: `CountInWindow { window_ms: 60000, threshold: 5 }`
13. High Memory Usage: `GreaterThan { path: "$.payload.memory_mb", value: 4096 }`
14. High Error Rate: `CountInWindow { window_ms: 300000, threshold: 50 }`
15. Queue Depth Warning: `GreaterThan { path: "$.payload.queue_depth", value: 1000 }`

### 7.3 Alert Classification

**Severity Levels & Escalation Paths:**
| Severity | Use Case | Escalation Path | Response Time |
|----------|-----------|-----------------|---------------|
| **Info** | Normal operations | Log signal only | N/A |
| **Low** | Minor issues | Notify supervisor (low priority) | 1 hour |
| **Medium** | Degraded performance | Notify supervisor (normal priority) | 30 minutes |
| **High** | Security concern, policy violation | Escalate to supervisor (high priority) + block action | 10 minutes |
| **Critical** | System failure, severe breach | Escalate to ApplicationSupervisor (critical) + immediate block | 5 minutes |

**Alert Structure:**
```rust
pub struct Alert {
    pub alert_id: String,
    pub rule_id: String,
    pub trigger_event: Event,
    pub severity: AlertSeverity,
    pub category: AlertCategory,
    pub message: String,
    pub triggered_at: DateTime<Utc>,
    pub acknowledged: bool,
    pub acknowledged_by: Option<String>,
    pub escalation_status: EscalationStatus,
    pub source: String,
    pub dedup_hash: String,
    pub suppressed_until: Option<DateTime<Utc>>,
}
```

### 7.4 Feedback Loop Prevention

**Four mechanisms to prevent infinite loops:**

**1. Event Depth Tracking:**
```rust
if event.event_depth > 5 {
    tracing::warn!("Event exceeded max depth, dropping");
    return Ok(());
}
event = event.increment_depth();
```

**2. Deduplication Hash:**
```rust
let dedup_hash = format!("{:x}", hasher.finish());
if state.dedup_cache.contains_key(&dedup_hash) {
    return Ok(());  // Duplicate, suppress
}
state.dedup_cache.insert(dedup_hash, Utc::now());
```

**3. Circuit Breaker:**
```rust
if state.circuit_breaker.failure_count > 100 {
    state.circuit_breaker.state = CircuitBreakerState::Open;
    return Err(WatcherError::CircuitBreakerOpen);
}
```

**4. Source Tagging:**
```rust
let alert_event = Event::new(
    EventType::Custom("alert.emitted".to_string()),
    "alert.emitted",
    serde_json::to_value(&alert).unwrap(),
    self.my_id(),
).unwrap()
.with_source_type("watcher".to_string())
.with_is_watcher_generated(true);
```

### 7.5 Watcher Output Events

Watcher emits these events to EventBus:

1. `alert.emitted` - Alert triggered
2. `rule.matched` - Rule condition matched
3. `alert.acknowledged` - Alert acknowledged by user
4. `escalation.sent` - Escalation sent to supervisor
5. `alert.suppressed` - Alert suppressed (dedup)
6. `circuit_breaker.tripped` - Circuit breaker triggered
7. `watcher.stats` - Watcher statistics snapshot
8. `rule.enabled` / `rule.disabled` - Rule state changed
9. `subscription.created` - New subscription created
10. `watcher.started` / `watcher.stopped` - Watcher lifecycle

**Escalation to Supervisor:**
```rust
match alert.severity {
    AlertSeverity::Critical => {
        // Escalate to ApplicationSupervisor
        ractor::cast!(
            application_supervisor,
            SupervisorMsg::CriticalAlert { alert: alert.clone() }
        );
    }
    AlertSeverity::High => {
        // Escalate to SessionSupervisor
        ractor::cast!(
            session_supervisor,
            SupervisorMsg::HighSeverityAlert { alert: alert.clone() }
        );
    }
    _ => {
        // Log signal only
        emit_log_signal(alert, LogLevel::Warn).await?;
    }
}
```

---

## 8. Logs App Evolution Roadmap

### Phase 1: Live Stream Console (2 weeks)

**Goal:** Show real-time event stream for a session

**Features:**
1. WebSocket connection to event stream
2. Auto-scrolling log view with latest events
3. Filter by interface kind (uActor / AppActor)
4. Color-coded event types
5. Basic search (text filter)
6. Pause/resume streaming

**UI Layout:**
```
┌─────────────────────────────────────────────────────────────┐
│  Filter: [uActor ▾] Search: [_____________] [Pause] │
├─────────────────────────────────────────────────────────────┤
│  [14:32:10] [uActor] delegation to SessionSupervisor│
│  [14:32:11] [uActor] delegation accepted         │
│  [14:32:12] [uActor] worker spawned (Nano)        │
│  [14:32:15] [BAML] model_invoke start (PlanAction) │
│  [14:32:16] [BAML] tool_extracted (bash)         │
│  [14:32:17] [Tool] bash command: [REDACTED]   │
│  ...                                                   │
└─────────────────────────────────────────────────────────────┘
```

**Backend Capabilities:**
- WebSocket endpoint: `ws://localhost:8080/ws/events?session_id={id}&thread_id={id}`
- Event query with `since_seq` for reconnection
- No new database indexes needed

**Risks:**
- None (simple, build on existing WebSocket infra)

---

### Phase 2: Filterable Trace Explorer (3 weeks)

**Goal:** Navigate traces with filters, drill-down into specific events

**Features:**
1. Trace view with parent/child tree
2. Filter by event type, actor, interface kind
3. Time range picker
4. Click event to show full payload
5. Expand/collapse tool call details
6. Show duration, latency, error info
7. Diff view for comparing traces

**UI Layout:**
```
┌─────────────────────────────────────────────────────────────┐
│  Filter: [Event Type ▾] [Actor ▾] [Interface ▾]   │
│  Time: [01/01/2026] to [01/31/2026]             │
├──────────────┬──────────────────────────────────────────────┤
│  Trace Tree  │  Event Details                            │
│              │                                       │
│ ┌──────────┐│  Event: appactor.tool_call_start         │
│ │Trace T1  ││  ID: call-456                           │
│ │├─Del1    ││  Timestamp: 2026-02-08T12:36:02Z     │
│ │├─Del2    ││  Tool: bash                              │
│ │├─Model    ││  Args:                                   │
│ │├─Tool1    ││  {                                      │
│ ││ └─Tool2 ││    "command": "ls -la",               │
│ │└─Result  ││    "cwd": "/tmp"                       │
│ └──────────┘│  }                                      │
│              │  Duration: 3350ms                      │
│              │  Status: Success                        │
│              │                                       │
└──────────────┴──────────────────────────────────────────────┘
```

**Backend Capabilities:**
- New query APIs for trace reconstruction
- Indexes on `trace_id`, `(actor_id, session_id, thread_id, seq)`
- Aggregation queries for statistics

**Risks:**
- UI complexity (need good UX)
- Query performance for large traces

---

### Phase 3: Dashboard + Concept Map (6 weeks)

**Goal:** Interactive visualization with concept-map navigation

**Features:**
1. Force-directed graph of actors, tools, files
2. Word-cloud sidebar of most-used tools/actors
3. Click concepts to filter timeline
4. Time-scrubber for historical state
5. Heatmap of activity over time
6. Alert summary panel
7. Metrics dashboard (latency, error rate, cost)

**UI Layout:**
```
┌─────────────────────────────────────────────────────────────┐
│  Metrics Dashboard                                        │
│  Latency (p95): 1.2s  Error Rate: 2.3%  Cost: $0.45 │
├────────────────┬─────────────────────────────────────────────┤
│ Word Cloud     │  Concept Map                          │
│                │                                       │
│   bash (47)    │      [ChatAgent]                      │
│  grep (32)     │        /    \                          │
│ read_file(28) │      /      \         [Tool:bash]     │
│ write_file(25)│    /          \       /       \        │
│                │ [TerminalActor]   [File:/tmp/data]    │
│                │                                     │
│ Time scrubber:│  ●━━━━━━━━━━━━━━━━━━━━━━━●           │
│   14:00     │                                       │
│      16:00    │                                       │
└───────────────┴───────────────────────────────────────────┘
```

**Backend Capabilities:**
- Concept aggregation API (counts by tool/actor/file)
- Time-series aggregation queries
- Alert statistics queries
- High-performance time-series indexes

**Risks:**
- WebGL rendering performance
- Concept map layout algorithm complexity
- Large dataset rendering

---

### Phase 4: Advanced Analytics (8 weeks, optional)

**Goal:** Eval support, run comparison, model/provider analysis

**Features:**
1. Compare two runs side-by-side
2. Model/provider performance metrics
3. Cost analysis by model/provider
4. A/B test frameworks for prompts
5. Replay with sandboxing
6. Export/import traces
7. ML-based anomaly detection

**UI Layout:**
```
┌─────────────────────────────────────────────────────────────┐
│  Run Comparison                                         │
├──────────────┬──────────────────────────────────────────────┤
│  Run A        │  Run B                                   │
│  Model: GPT-4  │  Model: Claude Opus                       │
│  Latency: 1.2s│  Latency: 0.8s                          │
│  Cost: $0.03   │  Cost: $0.02                             │
├──────────────┴──────────────────────────────────────────────┤
│  Diff View:                                             │
│  + Model Changed: GPT-4 → Claude Opus               │
│  + Tool Calls: 5 → 5                                │
│  - Latency: 1.2s → 0.8s (-33%)                  │
│  + Cost: $0.03 → $0.02 (-33%)                      │
└─────────────────────────────────────────────────────────────┘
```

**Backend Capabilities:**
- Run comparison queries
- Model/provider aggregation
- Replay execution with sandboxing
- Export API (JSON/CSV)
- ML model integration (optional)

**Risks:**
- Replay security (must sandbox)
- Complex query patterns
- Performance for large datasets

---

## 9. Concept-Map / Word-Cloud UX Research

### 9.1 Concept Definition

A **concept** is a first-class entity extracted from events:

| Concept Type | Definition | Extraction Source | Example |
|-------------|------------|------------------|---------|
| **Actor** | Processing entity that emits events | `actor_id` in events | `ChatActor`, `TerminalActor` |
| **Tool** | Action invoked by an agent | `appactor.tool_call_start` events | `bash`, `read_file`, `write_file` |
| **File** | Resource read/written | `appactor.file_read/write` events | `/tmp/data.txt`, `src/main.rs` |
| **Model** | LLM provider used | `baml.model_resolution` events | `ClaudeBedrockOpus45`, `ZaiGLM47` |
| **Session** | Scope boundary for multi-thread work | `session_id` in events | `session-123` |
| **Thread** | Conversation thread within session | `thread_id` in events | `thread-456` |

**Concept Properties:**
```rust
struct Concept {
    id: String,              // e.g., "tool:bash"
    kind: ConceptKind,       // Actor, Tool, File, Model
    display_name: String,     // e.g., "bash"
    first_seen: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    event_count: usize,
    related_concepts: Vec<ConceptId>,
    state: ConceptState,      // Active, Idle, Failed, Completed
}
```

### 9.2 Interaction Design

| Action | Effect | Feedback |
|--------|--------|----------|
| **Hover** | Highlight concept + 1-hop neighbors | Glow animation, tooltip with stats |
| **Click** | Select concept (pin to detail pane) | Concept expands, related events filter timeline |
| **Double-click** | Zoom to concept subgraph | View focuses on concept cluster |
| **Drag** | Pan graph view | Smooth pan with momentum |
| **Scroll** | Zoom in/out | Smooth zoom with level indicator |
| **Right-click** | Context menu (filter, export, hide) | Menu with actions |

### 9.3 Visual Layout: Force-Directed with Gravity Zones

**Hybrid layout:**
```
┌─────────────────────────────────────────────────────────────┐
│                    SUPERVISOR ZONE (top)                │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐               │
│  │ChatSup   │  │TermSup   │  │DeskSup   │               │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘               │
│       │             │             │                          │
├───────┼─────────────┼─────────────┼──────────────────────────┤
│       │             │             │                          │
│  ┌────▼─────┐  ┌──▼──────┐  ┌──▼──────┐               │
│  │ChatActor │  │TermActor│  │DeskActor│  ← ACTOR ZONE    │
│  └────┬─────┘  └──┬──────┘  └──┬──────┘               │
│       │             │             │                          │
├───────┼─────────────┼─────────────┼──────────────────────────┤
│       │             │             │                          │
│  ┌────▼─────┐  ┌──▼──────┐  ┌──▼──────┐               │
│  │Tool:bash │  │Tool:grep│  │File:rs  │  ← TOOL/FILE ZONE│
│  └──────────┘  └──────────┘  └──────────┘               │
└─────────────────────────────────────────────────────────────┘
```

**Gravity Rules:**
- Supervisors attracted to top edge
- Actors attracted to middle, grouped by parent supervisor
- Tools attracted to bottom, clustered by frequency
- Files orbit tools that modified them
- Edges apply weak spring forces between related concepts

### 9.4 Animation Strategy

**Time-Slice Playback Mode:**
- Timeline slider scrubs through time
- At each time point:
  1. Calculate `active_mask` for all concepts (events in last 5 seconds)
  2. Animate active concepts: expand size, pulse opacity
  3. Fade out inactive concepts toward baseline opacity
  4. Draw edges only for active concepts

**State Transition Animations:**
| Transition | Animation | Duration |
|------------|-----------|----------|
| Concept appears | Scale 0→1 with bounce easing | 300ms |
| Tool call → Result | Particle flow from Actor to Tool, wait, flow back | Duration + 500ms |
| Error occurs | Red flash ripple from concept | 600ms |
| Worker completes | Green glow spreads through cluster | 800ms |
| Filter applied | Non-matching concepts fade out, matching recluster | 500ms |

### 9.5 MVP Path

**Phase 1: Static Concept Map (2 weeks)**
1. Extract concepts from event log (actors, tools, files, models)
2. Build force-directed graph with basic layout
3. Render nodes (circles) with size based on event count
4. Render edges between co-occurring concepts
5. Hover shows concept type and event count
6. Click selects concept and shows detail pane

**Phase 2: Word Cloud Sidebar (1 week)**
1. Extract tool/actor frequency from events
2. Display as word cloud (size = frequency)
3. Click word → filter concept map to show only related concepts
4. Hover → show count and recent events

**Phase 3: Timeline Sync (1 week)**
1. Display timeline of all events
2. Select concept in map → highlight its events in timeline
3. Select events in timeline → flash related concepts in map
4. Time slider to scrub through trace

**Phase 4: Real-Time Streaming (2 weeks)**
1. Subscribe to WebSocket `Event` messages
2. Incrementally update concept map (new nodes/edges)
3. Animate new concept appearances
4. Optimize for 10K+ events (virtualized rendering, batching)

### 9.6 Anti-Patterns to Avoid

1. **Over-cluttering:** Show every event as a separate node. **Solution:** Aggregate events into concepts. Show only top concepts by activity.

2. **Meaningless Animations:** Constant spinning/pulsing with no semantic meaning. **Solution:** Animate only state transitions (appearing, error, completion).

3. **Poor Visual Hierarchy:** All nodes same size/color. **Solution:** Encode information in visual properties: size → event count, color → concept type.

4. **Blocking UI:** Layout simulation freezes browser for seconds. **Solution:** Run force simulation in Web Worker, incremental updates.

5. **Losing Temporal Context:** Static graph loses time information. **Solution:** Always show timeline slider. Use opacity to fade old concepts.

---

## 10. Implementation Checklist

### Phase 1: Foundation (Weeks 1-2)

**Event Schema & Storage:**
- [ ] Add trace context fields to `Event` struct (`trace_id`, `span_id`, `parent_span_id`, `causality_id`)
- [ ] Add classification fields (`interface_kind`, `event_kind`, `status`, `duration_ms`)
- [ ] Add BAML metadata fields (`model`, `provider`, `baml_function`, `policy_refs`)
- [ ] Add security fields (`security_labels`, `redaction_state`)
- [ ] Add integrity fields (`integrity_hash`, `previous_hash`, `integrity_salt`)
- [ ] Add `schema_version` field
- [ ] Database migration for new fields (additive, no version increment)
- [ ] Create indexes: `idx_trace_lookup`, `idx_time_range`, `idx_interface_kind`

**Redaction & Security:**
- [ ] Implement `Redactor` with PII/secrets regex patterns
- [ ] Add redaction to `EventStoreActor::handle_append` (before storage)
- [ ] Add security labels detection (email, phone, credit card, API keys)
- [ ] Implement RBAC roles (User, Auditor, Admin, System)
- [ ] Add access control checks to event retrieval APIs

**BAML Instrumentation:**
- [ ] Create `sandbox/src/baml_client/telemetry.rs` module
- [ ] Implement `emit_baml_event_non_blocking()` helper
- [ ] Add token pricing table to `model_config.rs`
- [ ] Add error classification functions
- [ ] Wrap `PlanAction.call()` in `chat_agent.rs` with event emission
- [ ] Wrap `SynthesizeResponse.call()` in `chat_agent.rs`
- [ ] Add tool extraction event emission in tool execution loop

**Watcher Actor:**
- [ ] Create `sandbox/src/actors/watcher.rs` module
- [ ] Implement `WatcherActor` with ractor pattern
- [ ] Implement subscription API (topic, pattern, actor-scoped)
- [ ] Implement rule AST and evaluation engine
- [ ] Add 15 built-in rules (5 security, 5 policy, 5 observability)
- [ ] Implement alert emission and escalation logic
- [ ] Add feedback loop prevention (event depth, dedup, circuit breaker)
- [ ] Implement state persistence (subscriptions, rules, alerts)

**Testing:**
- [ ] Unit tests for redaction logic
- [ ] Unit tests for error classification
- [ ] Unit tests for token pricing
- [ ] Unit tests for rule evaluation
- [ ] Integration test for full BAML call lifecycle
- [ ] Performance test (<5ms telemetry overhead)

---

### Phase 2: Observability (Weeks 3-4)

**Event Queries:**
- [ ] Implement trace reconstruction query (`WHERE trace_id = ?`)
- [ ] Implement time-series query (`WHERE timestamp BETWEEN ? AND ?`)
- [ ] Implement run comparison query (`UNION ALL` for two sessions)
- [ ] Implement aggregation queries (avg latency, error rate, cost)

**Watcher Enhanced:**
- [ ] Add pattern-based payload filtering (JSONPath)
- [ ] Add count-in-window rule evaluation
- [ ] Add ML-based PII detection (optional, future)
- [ ] Implement alert deduplication windows
- [ ] Add circuit breaker implementation
- [ ] Implement alert acknowledgment workflow

**UI Phase 1 (Live Stream):**
- [ ] WebSocket endpoint for scoped event streaming
- [ ] Auto-scrolling log view component
- [ ] Filter by interface kind
- [ ] Color-coded event types
- [ ] Basic text search
- [ ] Pause/resume streaming

**Testing:**
- [ ] Integration test for WebSocket streaming
- [ ] Load test (10K events/sec)
- [ ] Trace reconstruction test (verify parent/child relationships)
- [ ] Redaction correctness test (verify PII redacted)
- [ ] Access control test (verify RBAC enforcement)

---

### Phase 3: Hardening (Weeks 5-8)

**Encryption & Integrity:**
- [ ] Enable SQLCipher encryption on database
- [ ] Implement hash chain for tamper evidence
- [ ] Add integrity verification API
- [ ] Implement digital signatures for critical events
- [ ] Add TLS for API and WebSocket connections

**Retention & Compliance:**
- [ ] Implement automated retention enforcement job
- [ ] Add GDPR data export endpoint
- [ ] Implement GDPR right-to-be-forgotten workflow
- [ ] Add audit logging for log access
- [ ] Implement compliance report generation (GDPR, SOC2, PCI-DSS)

**Secure Replay:**
- [ ] Implement `ReplaySandbox` with command whitelisting
- [ ] Add execution context validation (same session/thread)
- [ ] Implement replay modes (DryRun, ReadOnly, FullExecution)
- [ ] Add replay audit logging

**UI Phase 2 (Trace Explorer):**
- [ ] Trace view with parent/child tree
- [ ] Event detail pane
- [ ] Filter by event type, actor, interface kind
- [ ] Time range picker
- [ ] Diff view for comparing traces
- [ ] Expand/collapse tool call details

**Testing:**
- [ ] Penetration testing (log injection, privilege escalation)
- [ ] Tampering detection test (modify DB, verify detection)
- [ ] Compliance audit test (GDPR, SOC2 checklist)
- [ ] Replay security test (verify sandboxing)
- [ ] Access control bypass test

---

### Phase 4: Advanced Features (Weeks 9-12, optional)

**Model/Provider Agnostic Eval:**
- [ ] Run comparison dashboard
- [ ] Model/provider performance metrics aggregation
- [ ] Cost analysis by model/provider
- [ ] A/B test framework for prompts
- [ ] Export/import traces for external analysis

**Concept Map Visualization:**
- [ ] Force-directed graph implementation (WebGL/Dioxus)
- [ ] Word cloud sidebar
- [ ] Timeline sync between map and events
- [ ] Time-scrubber for historical state
- [ ] Heatmap of activity over time
- [ ] Metrics dashboard panels

**ML-Based Anomaly Detection:**
- [ ] Integrate DistilBERT for NER (named entity recognition)
- [ ] Implement ML-based PII detection
- [ ] Anomaly detection for unusual patterns
- [ ] Adaptive alerting thresholds

**Testing:**
- [ ] End-to-end eval workflow test
- [ ] ML model performance test (precision/recall)
- [ ] Concept map rendering performance test (10K+ nodes)
- [ ] UX usability testing

---

## 11. Open Questions + Decisions Needed

### 11.1 Open Questions

**Architecture:**
1. **Q:** Should we make `trace_id` required for all events? **Decision:** Start optional (v2), make required in v3 with backfill
2. **Q:** Should `interface_kind` be an enum or string? **Decision:** Use enum for type safety, serialize as string
3. **Q:** Should we separate EventStoreActor by interface kind (uActor events table vs AppActor events table)? **Decision:** Keep unified table for cross-interface queries, use `interface_kind` filter

**Security:**
4. **Q:** Should we encrypt all payload fields or only sensitive ones? **Decision:** Encrypt only sensitive fields (text, content, cmd, args) using field-level encryption
5. **Q:** What encryption algorithm for database? **Decision:** SQLCipher with AES-256-GCM, master key from environment variable
6. **Q:** Should we store secrets (hashes only) or redact and store? **Decision:** Store hashes only for secrets, redact for PII

**Watcher:**
7. **Q:** Should we support ML-based rules in Phase 1? **Decision:** No, start deterministic only, ML in Phase 4
8. **Q:** What should be default event depth limit? **Decision:** 5 (configurable)
9. **Q:** Should watchers emit events that can trigger other watchers? **Decision:** No, prevent by `is_watcher_generated` tag

**UI:**
10. **Q:** Should concept map use WebGL or canvas? **Decision:** Start with canvas (simpler), upgrade to WebGL in Phase 4 if performance issues
11. **Q:** What should be default zoom level for concept map? **Decision:** Show top 200 concepts by event count, zoom to fit
12. **Q:** Should we show all concepts by default or filter by type? **Decision:** Default to Actor + Tool concepts only, add toggle for Files, Models

**Compliance:**
13. **Q:** What is the retention period for GDPR compliance? **Decision:** 30 days hot, 1 year cold, user can request deletion
14. **Q:** Should we implement full GDPR compliance (all 8 requirements) or just erasure? **Decision:** Start with erasure (Art. 17), add other requirements in Phase 3
15. **Q:** Should we support data portability (export in machine-readable format)? **Decision:** Yes, Phase 3

---

### 11.2 Recommended Default Choices

| Question | Recommendation | Rationale |
|----------|----------------|------------|
| `trace_id` required | Start optional (v2), required v3 | Migration path allows gradual rollout |
| Encryption algorithm | SQLCipher with AES-256-GCM | Industry standard, good performance |
| Secret storage | Hashes only | Never store plaintext secrets |
| Event depth limit | 5 (configurable) | Prevents feedback loops, configurable for debugging |
| ML-based rules | Deferred to Phase 4 | Start simple, add complexity gradually |
| Default concept map zoom | Top 200 concepts by count | Balance completeness vs performance |
| PII detection | Regex-based (Phase 1), ML-based (Phase 4) | Fast regex first, improve with ML later |
| Retention period | 30 days hot / 1 year cold | Balance observability vs storage cost |
| Replay mode | DryRun default, require explicit approval for FullExecution | Safe by design |

---

## Conclusion

This design provides a comprehensive, ChoirOS-tailored architecture for logging and watcher systems that:

1. **Separates concerns cleanly:** uActor → Actor (meta-coordination) vs AppActor → ToolActor (typed tool execution)
2. **Provides unified causality tracking:** W3C TraceContext-inspired envelope with trace/span context
3. **Supports practical implementation:** Builds incrementally on existing EventStoreActor foundation
4. **Enables enterprise-grade security:** Redaction, PII detection, RBAC, integrity checks, secure replay
5. **Scales to high-concurrency:** Hybrid storage (hot SQLite + cold JSONL), non-blocking event emission
6. **Provides rich observability:** BAML instrumentation, model/provider-agnostic eval support, comprehensive watcher rules
7. **Evolves incrementally:** Phase 1 live stream → Phase 2 trace explorer → Phase 3 concept-map visualization

The design is **implementable now** in Rust with the existing BAML and ractor infrastructure, with concrete schemas, JSONL examples, and a phased rollout plan that mitigates risks and delivers value early.

---

**Document Version:** 1.0  
**Last Updated:** 2026-02-08  
**Status:** Ready for Implementation Review
