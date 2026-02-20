# Watcher Actor Architecture - ChoirOS Event Monitoring System

**Date:** 2026-02-08  
**Version:** 1.0  
**Status:** Research Document

---

## Narrative Summary (1-minute read)

Watcher actors are the observability backbone of ChoirOS, monitoring event streams for security violations, policy breaches, and system health. They subscribe to filtered event streams, evaluate deterministic rules (or ML models), and emit alerts to Logs App and escalation signals to supervisors. Key challenge: preventing feedback loops where watcher alerts generate more events, creating cascading failures. Solution: event depth tracking, deduplication windows, circuit breakers, and source tagging.

---

## Table of Contents

1. [Context & Requirements](#context--requirements)
2. [Subscription API Design](#subscription-api-design)
3. [Rule DSL & AST Schema](#rule-dsl--ast-schema)
4. [Alert Classification Schema](#alert-classification-schema)
5. [Feedback Loop Prevention Strategy](#feedback-loop-prevention-strategy)
6. [Watcher Output Event Types](#watcher-output-event-types)
7. [State Persistence Strategy](#state-persistence-strategy)
8. [Performance Characteristics](#performance-characteristics)
9. [Implementation Examples](#implementation-examples)
10. [Phase 1 Prototype Scope](#phase-1-prototype-scope)

---

## Context & Requirements

### Current ChoirOS Event System

**EventBusActor** (`sandbox/src/actors/event_bus.rs`):
- Topic-based pub/sub using ractor Process Groups
- Wildcard pattern support: `"worker.*"` matches `"worker.task"`, `"worker.job"`
- Event structure: `{id, event_type, topic, payload, timestamp, source, correlation_id?}`
- Subscriber registration per topic with stats tracking

**EventStoreActor** (`sandbox/src/actors/event_store.rs`):
- Append-only event log with SQLite (sqlx)
- Sequence-numbered events (strict ordering)
- Scope isolation: `session_id`, `thread_id` columns prevent cross-instance bleed
- Query by actor, scope, or sequence range

**Supervision Tree**:
```
ApplicationSupervisor
  └── SessionSupervisor
      ├── ChatSupervisor
      ├── TerminalSupervisor
      └── DesktopSupervisor
```

### WatcherActor Requirements (from AGENTS.md)

**Primary Goal:** Prototype for timeout/failure escalation signals to supervisors.

**Use Cases:**
1. **Security Monitoring** - PII detection, suspicious command patterns
2. **Policy Enforcement** - Tool blocking, permission violations
3. **Observability Dashboards** - Slow model calls, worker failures
4. **Escalation to Supervisor** - Timeout signals, failure aggregates

**Constraints:**
- Must integrate with existing EventBusActor subscription model
- Must use ractor framework
- Must support deterministic rule evaluation (ML deferred)
- Must prevent feedback loops
- Target: 10K+ events/sec throughput

---

## Subscription API Design

### Overview

Watcher actors subscribe to filtered event streams via EventBusActor. Three subscription models:

1. **Topic-based**: Subscribe to exact topic or wildcard pattern
2. **Pattern-based**: Filter events by payload structure/value using JSONPath
3. **Actor-scoped**: Subscribe to events from specific actor IDs

### API Design

```rust
// sandbox/src/actors/watcher.rs

use ractor::{Actor, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Subscription filter specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherSubscription {
    /// Topic pattern (supports wildcards: "worker.*", "*")
    pub topic: String,

    /// Optional actor ID filter (only events from this actor)
    pub actor_id: Option<String>,

    /// Optional scope filters (session/thread isolation)
    pub scope: Option<ScopeFilter>,

    /// Optional payload JSONPath filters
    pub payload_filters: Vec<PayloadFilter>,

    /// Maximum events per second (rate limiting)
    pub max_events_per_sec: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeFilter {
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayloadFilter {
    /// JSONPath expression (e.g., "$.command", "$.tool")
    pub path: String,

    /// Exact match, regex, or numeric comparison
    pub condition: FilterCondition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FilterCondition {
    Equals { value: serde_json::Value },
    Contains { value: String },
    Regex { pattern: String },
    GreaterThan { value: f64 },
    LessThan { value: f64 },
    Exists { path: String },
}

/// Messages handled by WatcherActor
#[derive(Debug)]
pub enum WatcherMsg {
    /// Subscribe to event stream
    Subscribe {
        subscription: WatcherSubscription,
        rules: Vec<WatchRule>,
        reply: RpcReplyPort<Result<SubscriptionId, WatcherError>>,
    },

    /// Unsubscribe from event stream
    Unsubscribe {
        subscription_id: SubscriptionId,
        reply: RpcReplyPort<Result<(), WatcherError>>,
    },

    /// Event received from EventBusActor
    EventReceived {
        event: Event,
    },

    /// Query active alerts
    GetActiveAlerts {
        reply: RpcReplyPort<Vec<Alert>>,
    },

    /// Acknowledge alert (stop escalation)
    AcknowledgeAlert {
        alert_id: String,
        reply: RpcReplyPort<Result<(), WatcherError>>,
    },

    /// Get watcher stats
    GetStats {
        reply: RpcReplyPort<WatcherStats>,
    },
}

pub type SubscriptionId = String;
```

### Subscription Registration Flow

```rust
// WatcherActor::handle_subscribe
async fn handle_subscribe(
    &self,
    subscription: WatcherSubscription,
    rules: Vec<WatchRule>,
    reply: RpcReplyPort<Result<SubscriptionId, WatcherError>>,
    state: &mut WatcherState,
) -> Result<(), ActorProcessingErr> {
    let subscription_id = ulid::Ulid::new().to_string();

    // 1. Register with EventBusActor
    let event_bus = state.event_bus.clone();
    let myself = state.myself.clone();
    let topic = subscription.topic.clone();

    let subscribe_result = ractor::cast!(
        event_bus,
        EventBusMsg::Subscribe {
            topic: topic.clone(),
            subscriber: myself,
        }
    );

    if let Err(e) = subscribe_result {
        let _ = reply.send(Err(WatcherError::EventBusError(e.to_string())));
        return Ok(());
    }

    // 2. Store subscription state
    state.subscriptions.insert(
        subscription_id.clone(),
        ActiveSubscription {
            subscription,
            rules,
            created_at: Utc::now(),
            event_count: 0,
        },
    );

    let _ = reply.send(Ok(subscription_id));
    Ok(())
}
```

### Pattern-Based Filtering Examples

```rust
// Example: Subscribe to all bash commands containing "rm"
let subscription = WatcherSubscription {
    topic: "terminal.command".to_string(),
    actor_id: None,
    scope: None,
    payload_filters: vec![
        PayloadFilter {
            path: "$.command".to_string(),
            condition: FilterCondition::Contains {
                value: "rm".to_string(),
            },
        },
    ],
    max_events_per_sec: Some(100),
};

// Example: Subscribe to model calls slower than 5 seconds
let subscription = WatcherSubscription {
    topic: "model.call".to_string(),
    actor_id: None,
    scope: None,
    payload_filters: vec![
        PayloadFilter {
            path: "$.duration_ms".to_string(),
            condition: FilterCondition::GreaterThan { value: 5000.0 },
        },
    ],
    max_events_per_sec: None,
};

// Example: Subscribe to events from specific session
let subscription = WatcherSubscription {
    topic: "*".to_string(), // All topics
    actor_id: None,
    scope: Some(ScopeFilter {
        session_id: Some("session-123".to_string()),
        thread_id: None,
    }),
    payload_filters: vec![],
    max_events_per_sec: Some(1000),
};
```

---

## Rule DSL & AST Schema

### Overview

Watcher rules are **deterministic** rule-based checks. ML-based evaluation is deferred to post-deployment hardening phase. Rules are compiled to AST for fast evaluation.

### Rule DSL (Text Representation)

```text
# Example 1: PII detection rule
rule detect_pii_email {
    topic: "chat.*"
    filter: "$.message contains regex '[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}'"
    severity: high
    category: security
    alert: "PII detected: email address in chat message"
    dedup_window: 60s
}

# Example 2: Dangerous tool block
rule block_dangerous_command {
    topic: "terminal.command"
    filter: "$.command matches '^(rm -rf /|dd if=.*of=/dev/)"
    severity: critical
    category: policy
    action: block
    alert: "Dangerous command blocked: {command}"
    escalate_to: "supervisor"
}

# Example 3: Slow model call
rule slow_model_call {
    topic: "model.call.complete"
    filter: "$.duration_ms > 10000"
    severity: medium
    category: observability
    alert: "Slow model call: {model} took {duration_ms}ms"
    suppression_window: 300s
}

# Example 4: Worker failure rate
rule worker_failure_spike {
    topic: "worker.failed"
    filter: "count_in_window(60s) > 5"
    severity: high
    category: observability
    alert: "Worker failure spike: {count} failures in 60s"
    escalate_to: "supervisor"
}
```

### AST Schema (Internal Representation)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchRule {
    /// Unique rule identifier
    pub rule_id: String,

    /// Display name
    pub name: String,

    /// Rule description
    pub description: String,

    /// Compiled AST for evaluation
    pub ast: RuleAst,

    /// Alert configuration
    pub alert_config: AlertConfig,

    /// Action configuration (block, notify, escalate)
    pub action_config: ActionConfig,

    /// Whether rule is enabled
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RuleAst {
    /// Single field condition
    FieldCondition {
        path: String, // JSONPath
        operator: ComparisonOperator,
        value: serde_json::Value,
    },

    /// Logical AND
    And {
        conditions: Vec<RuleAst>,
    },

    /// Logical OR
    Or {
        conditions: Vec<RuleAst>,
    },

    /// Logical NOT
    Not {
        condition: Box<RuleAst>,
    },

    /// Count events in time window
    CountInWindow {
        window_ms: u64,
        threshold: usize,
    },

    /// Regex match
    RegexMatch {
        path: String,
        pattern: String,
    },

    /// Custom function call (extensibility)
    FunctionCall {
        function: String,
        args: Vec<serde_json::Value>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Contains,
    Matches,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    /// Alert severity
    pub severity: AlertSeverity,

    /// Alert category
    pub category: AlertCategory,

    /// Alert message template (supports {field} substitution)
    pub message_template: String,

    /// Deduplication window (suppress duplicates for N seconds)
    pub dedup_window_ms: u64,

    /// Suppression window (suppress all alerts for N seconds after trigger)
    pub suppression_window_ms: u64,

    /// Maximum alerts per window (prevents alert storms)
    pub max_alerts_per_window: usize,
    pub window_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertCategory {
    Security,
    Policy,
    Observability,
    Performance,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActionConfig {
    /// Only emit alert, no action
    Notify,

    /// Block the operation (e.g., prevent tool execution)
    Block {
        reason: String,
    },

    /// Escalate to supervisor
    Escalate {
        target: String, // "supervisor", "session_supervisor"
        priority: EscalationPriority,
    },

    /// Send signal to Logs App
    LogSignal {
        level: LogLevel,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscalationPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}
```

### 15 Example Rules

#### Security Rules (5)

```rust
// Rule 1: PII - Email Detection
RuleAst::RegexMatch {
    path: "$.message".to_string(),
    pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}".to_string(),
}

// Rule 2: PII - Phone Number Detection
RuleAst::RegexMatch {
    path: "$.message".to_string(),
    pattern: r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b".to_string(),
}

// Rule 3: PII - Credit Card Detection
RuleAst::RegexMatch {
    path: "$.message".to_string(),
    pattern: r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b".to_string(),
}

// Rule 4: SQL Injection Pattern
RuleAst::Or {
    conditions: vec![
        RuleAst::FieldCondition {
            path: "$.command".to_string(),
            operator: ComparisonOperator::Contains,
            value: serde_json::json!("' OR '1'='1"),
        },
        RuleAst::FieldCondition {
            path: "$.command".to_string(),
            operator: ComparisonOperator::Contains,
            value: serde_json::json!("'; DROP TABLE"),
        },
    ],
}

// Rule 5: Command Injection Pattern
RuleAst::FieldCondition {
    path: "$.command".to_string(),
    operator: ComparisonOperator::Contains,
    value: serde_json::json!("&& rm -rf /"),
}
```

#### Policy Rules (5)

```rust
// Rule 6: Block Dangerous Commands
RuleAst::Or {
    conditions: vec![
        RuleAst::FieldCondition {
            path: "$.command".to_string(),
            operator: ComparisonOperator::Contains,
            value: serde_json::json!("rm -rf /"),
        },
        RuleAst::FieldCondition {
            path: "$.command".to_string(),
            operator: ComparisonOperator::Contains,
            value: serde_json::json!("dd if=/dev/zero"),
        },
    ],
}

// Rule 7: Tool Usage Rate Limit
RuleAst::CountInWindow {
    window_ms: 60_000, // 1 minute
    threshold: 100,
}

// Rule 8: File Access Restriction
RuleAst::FieldCondition {
    path: "$.path".to_string(),
    operator: ComparisonOperator::Contains,
    value: serde_json::json!("/etc/passwd"),
}

// Rule 9: Network Access Restriction
RuleAst::FieldCondition {
    path: "$.url".to_string(),
    operator: ComparisonOperator::Contains,
    value: serde_json::json!("192.168."),
}

// Rule 10: Session Timeout
RuleAst::FieldCondition {
    path: "$.idle_duration_ms".to_string(),
    operator: ComparisonOperator::GreaterThan,
    value: serde_json::json!(30 * 60 * 1000), // 30 minutes
}
```

#### Observability Rules (5)

```rust
// Rule 11: Slow Model Call
RuleAst::FieldCondition {
    path: "$.duration_ms".to_string(),
    operator: ComparisonOperator::GreaterThan,
    value: serde_json::json!(10000),
}

// Rule 12: Worker Failure Rate
RuleAst::CountInWindow {
    window_ms: 60_000,
    threshold: 5,
}

// Rule 13: High Memory Usage
RuleAst::FieldCondition {
    path: "$.memory_mb".to_string(),
    operator: ComparisonOperator::GreaterThan,
    value: serde_json::json!(4096),
}

// Rule 14: High Error Rate
RuleAst::CountInWindow {
    window_ms: 300_000, // 5 minutes
    threshold: 50,
}

// Rule 15: Queue Depth Warning
RuleAst::FieldCondition {
    path: "$.queue_depth".to_string(),
    operator: ComparisonOperator::GreaterThan,
    value: serde_json::json!(1000),
}
```

---

## Alert Classification Schema

### Alert Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Unique alert ID
    pub alert_id: String,

    /// Associated rule ID
    pub rule_id: String,

    /// Triggering event
    pub trigger_event: Event,

    /// Alert severity
    pub severity: AlertSeverity,

    /// Alert category
    pub category: AlertCategory,

    /// Human-readable message
    pub message: String,

    /// When alert was triggered
    pub triggered_at: DateTime<Utc>,

    /// Whether alert has been acknowledged
    pub acknowledged: bool,

    /// Acknowledged by (if applicable)
    pub acknowledged_by: Option<String>,

    /// Escalation status
    pub escalation_status: EscalationStatus,

    /// Source of alert (actor ID)
    pub source: String,

    /// Alert deduplication hash
    pub dedup_hash: String,

    /// Suppression window end time
    pub suppressed_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EscalationStatus {
    None,
    Pending,
    Escalated,
    Acknowledged,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertMetadata {
    /// Deduplication window (ms)
    pub dedup_window_ms: u64,

    /// Suppression window (ms)
    pub suppression_window_ms: u64,

    /// Maximum alerts per window
    pub max_alerts_per_window: usize,

    /// Alert count in current window
    pub alert_count_in_window: usize,

    /// Window start time
    pub window_start: DateTime<Utc>,
}
```

### Severity Levels & Escalation Paths

| Severity | Use Case | Escalation Path | Response Time |
|----------|-----------|-----------------|---------------|
| **Info** | Normal operations, informational | Log signal only | N/A |
| **Low** | Minor issues, self-healing possible | Notify supervisor (low priority) | 1 hour |
| **Medium** | Degraded performance, needs attention | Notify supervisor (normal priority) | 30 minutes |
| **High** | Security concern, policy violation | Escalate to supervisor (high priority) + block action | 10 minutes |
| **Critical** | System failure, severe security breach | Escalate to ApplicationSupervisor (critical) + immediate block | 5 minutes |

### Escalation Strategy

```rust
impl WatcherActor {
    async fn escalate_alert(
        &self,
        alert: Alert,
        state: &mut WatcherState,
    ) -> Result<(), WatcherError> {
        match (alert.severity, alert.escalation_status) {
            (AlertSeverity::Critical, EscalationStatus::Pending) => {
                // Critical: Escalate to ApplicationSupervisor immediately
                if let Some(ref app_supervisor) = state.application_supervisor {
                    let _ = ractor::cast!(
                        app_supervisor,
                        SupervisorMsg::CriticalAlert {
                            alert: alert.clone(),
                        }
                    );
                }
            }
            (AlertSeverity::High, EscalationStatus::Pending) => {
                // High: Escalate to SessionSupervisor
                if let Some(ref session_supervisor) = state.session_supervisor {
                    let _ = ractor::cast!(
                        session_supervisor,
                        SupervisorMsg::HighSeverityAlert {
                            alert: alert.clone(),
                        }
                    );
                }
            }
            (AlertSeverity::Medium, EscalationStatus::Pending) => {
                // Medium: Notify supervisor via log signal
                self.emit_log_signal(alert, LogLevel::Warn).await?;
            }
            (AlertSeverity::Low, EscalationStatus::Pending) => {
                // Low: Notify supervisor via log signal
                self.emit_log_signal(alert, LogLevel::Info).await?;
            }
            _ => {
                // Already handled or info level
            }
        }
        Ok(())
    }
}
```

---

## Feedback Loop Prevention Strategy

### The Feedback Loop Problem

**Scenario:** Watcher detects PII in chat message → Emits alert event → Alert event contains PII → Watcher detects PII in alert → Infinite loop.

### Prevention Mechanisms

#### 1. Event Depth Tracking

Every event carries an `event_depth` field, incremented when passing through watchers.

```rust
impl Event {
    pub fn with_depth(mut self, depth: u32) -> Self {
        self.event_depth = depth;
        self
    }

    pub fn increment_depth(self) -> Self {
        self.with_depth(self.event_depth + 1)
    }
}

impl WatcherActor {
    async fn handle_event(
        &self,
        mut event: Event,
        state: &mut WatcherState,
    ) -> Result<(), ActorProcessingErr> {
        // Reject events beyond depth threshold (default: 5)
        if event.event_depth > state.config.max_event_depth {
            tracing::warn!(
                event_id = %event.id,
                depth = event.event_depth,
                "Event exceeded max depth, dropping"
            );
            return Ok(());
        }

        // Increment depth for downstream events
        event = event.increment_depth();
        // ... process event
    }
}
```

#### 2. Deduplication Hash

Compute hash of (rule_id + normalized_event_fields) to prevent duplicate alerts.

```rust
impl WatcherActor {
    fn compute_dedup_hash(&self, rule_id: &str, event: &Event) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        rule_id.hash(&mut hasher);

        // Normalize: ignore timestamps, sequence numbers
        let normalized = serde_json::json!({
            "event_type": event.event_type,
            "topic": event.topic,
            "payload": self.normalize_payload(&event.payload),
        });
        normalized.to_string().hash(&mut hasher);

        format!("{:x}", hasher.finish())
    }

    fn normalize_payload(&self, payload: &serde_json::Value) -> serde_json::Value {
        // Remove timestamps, IDs, etc.
        // Implement field-wise normalization
        payload.clone()
    }
}
```

#### 3. Deduplication Window

Track recent alerts by dedup_hash, suppress within window.

```rust
pub struct WatcherState {
    // ... other fields
    dedup_cache: LruCache<String, DateTime<Utc>>,
    config: WatcherConfig,
}

impl WatcherActor {
    async fn check_dedup(
        &self,
        dedup_hash: &str,
        dedup_window_ms: u64,
        state: &mut WatcherState,
    ) -> bool {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::milliseconds(dedup_window_ms as i64);

        if let Some(last_seen) = state.dedup_cache.get(dedup_hash) {
            if *last_seen > cutoff {
                return true; // Duplicate alert, suppress
            }
        }

        state.dedup_cache.put(dedup_hash.to_string(), now);
        false // Not a duplicate
    }
}
```

#### 4. Circuit Breaker

Stop processing events if alert rate exceeds threshold.

```rust
pub struct CircuitBreaker {
    failure_count: usize,
    last_failure_time: DateTime<Utc>,
    state: CircuitBreakerState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitBreakerState {
    Closed,   // Normal operation
    Open,     // Rejecting events
    HalfOpen, // Testing recovery
}

impl WatcherActor {
    async fn check_circuit_breaker(
        &self,
        state: &mut WatcherState,
    ) -> Result<(), WatcherError> {
        let breaker = &mut state.circuit_breaker;

        match breaker.state {
            CircuitBreakerState::Closed => {
                // Check if we should open
                if breaker.failure_count > state.config.max_failures {
                    breaker.state = CircuitBreakerState::Open;
                    breaker.last_failure_time = Utc::now();
                    return Err(WatcherError::CircuitBreakerOpen);
                }
            }
            CircuitBreakerState::Open => {
                // Check if we should try recovery
                let cooldown = chrono::Duration::milliseconds(
                    state.config.circuit_breaker_cooldown_ms as i64,
                );
                if Utc::now() - breaker.last_failure_time > cooldown {
                    breaker.state = CircuitBreakerState::HalfOpen;
                    breaker.failure_count = 0;
                } else {
                    return Err(WatcherError::CircuitBreakerOpen);
                }
            }
            CircuitBreakerState::HalfOpen => {
                // Allow limited traffic to test
                if breaker.failure_count > 0 {
                    breaker.state = CircuitBreakerState::Open;
                    breaker.last_failure_time = Utc::now();
                    return Err(WatcherError::CircuitBreakerOpen);
                }
            }
        }

        Ok(())
    }
}
```

#### 5. Source Tagging

Tag events generated by watchers to prevent self-monitoring.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    // ... existing fields
    /// Event source type
    pub source_type: Option<String>, // "watcher", "actor", "external"

    /// Whether this event is from a watcher (prevents feedback loops)
    pub is_watcher_generated: bool,
}

impl WatcherActor {
    fn emit_alert_event(
        &self,
        alert: Alert,
    ) -> Event {
        Event::new(
            EventType::Custom("alert.emitted".to_string()),
            "alert.emitted",
            serde_json::to_value(&alert).unwrap(),
            self.my_id(),
        )
        .unwrap()
        .with_source_type("watcher".to_string())
        .with_is_watcher_generated(true)
    }
}
```

---

## Watcher Output Event Types

### Alert Emitted Event

```rust
Event {
    event_type: "alert.emitted",
    topic: "alert.emitted",
    payload: {
        "alert_id": "alert-123",
        "rule_id": "rule-pii-email",
        "severity": "high",
        "category": "security",
        "message": "PII detected: email address in chat message",
        "source": "watcher-security-1",
    },
    source_type: "watcher",
    is_watcher_generated: true,
}
```

### Rule Matched Event

```rust
Event {
    event_type: "rule.matched",
    topic: "rule.matched",
    payload: {
        "rule_id": "rule-slow-model",
        "event_id": "evt-456",
        "match_details": {
            "duration_ms": 12500,
            "threshold": 10000,
        },
    },
    source_type: "watcher",
}
```

### Alert Acknowledged Event

```rust
Event {
    event_type: "alert.acknowledged",
    topic: "alert.acknowledged",
    payload: {
        "alert_id": "alert-123",
        "acknowledged_by": "user-1",
        "acknowledged_at": "2026-02-08T10:30:00Z",
    },
    source_type: "watcher",
}
```

### Escalation Sent Event

```rust
Event {
    event_type: "escalation.sent",
    topic: "escalation.sent",
    payload: {
        "alert_id": "alert-123",
        "target": "session_supervisor",
        "priority": "high",
        "sent_at": "2026-02-08T10:30:00Z",
    },
    source_type: "watcher",
}
```

### Alert Suppressed Event

```rust
Event {
    event_type: "alert.suppressed",
    topic: "alert.suppressed",
    payload: {
        "alert_id": "alert-123",
        "reason": "deduplication",
        "dedup_hash": "abc123",
        "suppressed_until": "2026-02-08T10:31:00Z",
    },
    source_type: "watcher",
}
```

### Circuit Breaker Tripped Event

```rust
Event {
    event_type: "circuit_breaker.tripped",
    topic: "system.circuit_breaker",
    payload: {
        "watcher_id": "watcher-security-1",
        "state": "open",
        "failure_count": 101,
        "threshold": 100,
        "triggered_at": "2026-02-08T10:30:00Z",
    },
    source_type: "watcher",
}
```

### Statistic Snapshot Event

```rust
Event {
    event_type: "watcher.stats",
    topic: "watcher.stats",
    payload: {
        "watcher_id": "watcher-security-1",
        "events_processed": 15234,
        "alerts_emitted": 123,
        "alerts_suppressed": 45,
        "current_subscription_count": 5,
        "avg_processing_time_ms": 2.5,
    },
    source_type: "watcher",
}
```

### Rule Enabled/Disabled Event

```rust
Event {
    event_type: "rule.enabled",
    topic: "rule.state_change",
    payload: {
        "rule_id": "rule-pii-email",
        "enabled": true,
        "changed_by": "user-1",
        "changed_at": "2026-02-08T10:30:00Z",
    },
    source_type: "watcher",
}
```

### Subscription Created Event

```rust
Event {
    event_type: "subscription.created",
    topic: "subscription.created",
    payload: {
        "subscription_id": "sub-123",
        "topic": "chat.*",
        "rule_count": 3,
        "created_at": "2026-02-08T10:30:00Z",
    },
    source_type: "watcher",
}
```

### Watcher Started/Stopped Event

```rust
Event {
    event_type: "watcher.started",
    topic: "watcher.lifecycle",
    payload: {
        "watcher_id": "watcher-security-1",
        "config": {
            "max_event_depth": 5,
            "max_failures": 100,
            "circuit_breaker_cooldown_ms": 60000,
        },
        "started_at": "2026-02-08T10:30:00Z",
    },
    source_type: "watcher",
}
```

---

## State Persistence Strategy

### Ephemeral vs Persistent State

**Ephemeral State (In-Memory Only):**
- Deduplication cache (LruCache)
- Circuit breaker state
- Recent event buffer
- Alert count in window

**Persistent State (To EventStore):**
- Active subscriptions
- Enabled rules
- Active alerts (with acknowledgment status)
- Watcher statistics

### Persistence Schema

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherStateSnapshot {
    /// Watcher ID
    pub watcher_id: String,

    /// Last sequence number processed
    pub last_seq: i64,

    /// Active subscriptions
    pub subscriptions: Vec<ActiveSubscription>,

    /// Enabled rules
    pub rules: Vec<WatchRule>,

    /// Active alerts
    pub alerts: Vec<Alert>,

    /// Statistics snapshot
    pub stats: WatcherStats,

    /// Snapshot timestamp
    pub snapshot_at: DateTime<Utc>,
}

impl WatcherActor {
    async fn persist_state(&self, state: &WatcherState) -> Result<(), WatcherError> {
        let snapshot = WatcherStateSnapshot {
            watcher_id: state.watcher_id.clone(),
            last_seq: state.last_seq,
            subscriptions: state.subscriptions.values().cloned().collect(),
            rules: state.rules.values().cloned().collect(),
            alerts: state.alerts.values().cloned().collect(),
            stats: state.stats.clone(),
            snapshot_at: Utc::now(),
        };

        let event = AppendEvent::new(
            "watcher.state_snapshot",
            snapshot,
            state.watcher_id.clone(),
            "system".to_string(),
        )?;

        let event_store = state.event_store.clone();
        ractor::call!(event_store, |reply| EventStoreMsg::Append {
            event,
            reply,
        })??;

        Ok(())
    }

    async fn restore_state(&self, state: &mut WatcherState) -> Result<(), WatcherError> {
        // Query last 10 state snapshots
        let event_store = state.event_store.clone();
        let events = ractor::call!(event_store, |reply| EventStoreMsg::GetEventsForActor {
            actor_id: state.watcher_id.clone(),
            since_seq: 0,
            reply,
        })??;

        // Find latest state snapshot
        let latest_snapshot = events.iter()
            .filter(|e| e.event_type == "watcher.state_snapshot")
            .last();

        if let Some(event) = latest_snapshot {
            let snapshot: WatcherStateSnapshot = serde_json::from_value(event.payload.clone())?;
            self.apply_snapshot(snapshot, state).await?;
        }

        Ok(())
    }
}
```

### Crash Recovery Flow

```rust
impl WatcherActor {
    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        tracing::info!("WatcherActor starting, restoring state...");

        let mut state = WatcherState::new(args);

        // 1. Restore persistent state from EventStore
        if let Err(e) = self.restore_state(&mut state).await {
            tracing::error!("Failed to restore state: {}", e);
            // Fall back to default state
        }

        // 2. Re-subscribe to all topics
        for (sub_id, sub) in state.subscriptions.iter() {
            if let Err(e) = self.resubscribe(sub_id, sub, &mut state).await {
                tracing::error!("Failed to resubscribe {}: {}", sub_id, e);
            }
        }

        // 3. Resume event processing from last_seq
        tracing::info!("WatcherActor restored, last_seq={}", state.last_seq);

        Ok(state)
    }
}
```

---

## Performance Characteristics

### Target Performance

| Metric | Target | Rationale |
|--------|--------|-----------|
| **Events/sec** | 10,000+ | Peak load in production environment |
| **Alert latency** | < 100ms | From event receipt to alert emission |
| **Memory footprint** | < 100MB | Per watcher instance |
| **Dedup cache** | 10,000 entries | Configurable LRU cache |
| **Circuit breaker threshold** | 100 failures/minute | Prevents cascading failures |

### Performance Optimization Strategies

#### 1. Batching Events

```rust
impl WatcherActor {
    async fn process_events_batch(&self, events: Vec<Event>, state: &mut WatcherState) {
        for event in events {
            if let Err(e) = self.check_circuit_breaker(state).await {
                tracing::warn!("Circuit breaker open: {}", e);
                break;
            }
            self.handle_event(event, state).await;
        }
    }
}
```

#### 2. Async Rule Evaluation

```rust
impl WatcherActor {
    async fn evaluate_rules_parallel(
        &self,
        event: &Event,
        rules: &[WatchRule],
    ) -> Vec<RuleEvaluationResult> {
        let futures: Vec<_> = rules
            .iter()
            .map(|rule| self.evaluate_rule(event, rule))
            .collect();

        futures::future::join_all(futures).await
    }
}
```

#### 3. Memory-Matched Regex Pre-compilation

```rust
impl WatcherActor {
    fn precompile_regexes(&self, rules: &[WatchRule]) -> Result<(), WatcherError> {
        for rule in rules {
            if let RuleAst::RegexMatch { pattern, .. } = &rule.ast {
                let regex = Regex::new(pattern)?;
                state.regex_cache.insert(rule.rule_id.clone(), regex);
            }
        }
        Ok(())
    }
}
```

#### 4. LRU Cache for Deduplication

```rust
pub struct WatcherState {
    // ... other fields
    dedup_cache: LruCache<String, DateTime<Utc>>,
}

impl WatcherState {
    pub fn new(config: WatcherConfig) -> Self {
        Self {
            dedup_cache: LruCache::new(config.dedup_cache_capacity),
            // ... other fields
        }
    }
}
```

### Performance Monitoring

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatcherStats {
    /// Total events processed
    pub events_processed: u64,

    /// Total alerts emitted
    pub alerts_emitted: u64,

    /// Total alerts suppressed (dedup)
    pub alerts_suppressed: u64,

    /// Current subscription count
    pub subscription_count: usize,

    /// Average processing time (ms)
    pub avg_processing_time_ms: f64,

    /// Circuit breaker trip count
    pub circuit_breaker_trips: u64,

    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
}

impl WatcherActor {
    fn update_stats(&mut self, processing_time_ms: u64, state: &mut WatcherState) {
        let stats = &mut state.stats;
        stats.events_processed += 1;

        // Rolling average
        let alpha = 0.1; // Smoothing factor
        stats.avg_processing_time_ms = alpha * processing_time_ms as f64
            + (1.0 - alpha) * stats.avg_processing_time_ms;

        stats.last_updated = Utc::now();
    }
}
```

---

## Implementation Examples

### Example 1: Security Alert (PII Detected)

```rust
// Event received from EventBusActor
Event {
    id: "evt-789",
    event_type: "chat.user_msg",
    topic: "chat.user_msg",
    payload: {
        "text": "Please email support@example.com for help",
        "scope": { "session_id": "session-123", "thread_id": "thread-456" },
    },
    source: "chat-1",
    event_depth: 0,
}

// Rule evaluation
RuleAst::RegexMatch {
    path: "$.text".to_string(),
    pattern: r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}".to_string(),
}

// Result: Alert emitted
Alert {
    alert_id: "alert-pii-001",
    rule_id: "rule-pii-email",
    severity: AlertSeverity::High,
    category: AlertCategory::Security,
    message: "PII detected: email address in chat message",
    triggered_at: Utc::now(),
    escalation_status: EscalationStatus::Pending,
    dedup_hash: "a1b2c3d4",
}

// Watcher emits alert event
Event {
    event_type: "alert.emitted",
    topic: "alert.emitted",
    payload: { /* alert data */ },
    source_type: "watcher",
    is_watcher_generated: true,
    event_depth: 1, // Incremented
}
```

### Example 2: Policy Violation (Tool Blocked)

```rust
// Event received from EventBusActor
Event {
    id: "evt-790",
    event_type: "terminal.command",
    topic: "terminal.command",
    payload: {
        "command": "rm -rf /home/user/docs",
        "terminal_id": "term-1",
    },
    source: "terminal-1",
    event_depth: 0,
}

// Rule evaluation
RuleAst::FieldCondition {
    path: "$.command".to_string(),
    operator: ComparisonOperator::Contains,
    value: serde_json::json!("rm -rf /"),
}

// Result: Alert + Block action
Alert {
    alert_id: "alert-danger-001",
    rule_id: "rule-dangerous-command",
    severity: AlertSeverity::Critical,
    category: AlertCategory::Policy,
    message: "Dangerous command blocked: rm -rf /home/user/docs",
    action_config: ActionConfig::Block {
        reason: "Potential data loss prevented",
    },
}

// Watcher escalates to supervisor
ractor::cast!(
    session_supervisor,
    SupervisorMsg::CriticalAlert {
        alert: alert.clone(),
    }
)
```

### Example 3: Observability Signal (Slow Model Call)

```rust
// Event received from EventBusActor
Event {
    id: "evt-791",
    event_type: "model.call.complete",
    topic: "model.call.complete",
    payload: {
        "model": "gpt-4",
        "duration_ms": 12500,
        "tokens": 500,
    },
    source: "baml-client",
    event_depth: 0,
}

// Rule evaluation
RuleAst::FieldCondition {
    path: "$.duration_ms".to_string(),
    operator: ComparisonOperator::GreaterThan,
    value: serde_json::json!(10000),
}

// Result: Observability alert (no escalation)
Alert {
    alert_id: "alert-slow-001",
    rule_id: "rule-slow-model",
    severity: AlertSeverity::Medium,
    category: AlertCategory::Observability,
    message: "Slow model call: gpt-4 took 12500ms",
    action_config: ActionConfig::LogSignal {
        level: LogLevel::Warn,
    },
}

// Watcher emits log signal to Logs App
Event {
    event_type: "log.signal",
    topic: "logs.app",
    payload: {
        "level": "warn",
        "message": "Slow model call: gpt-4 took 12500ms",
        "timestamp": Utc::now(),
    },
    source_type: "watcher",
}
```

### Example 4: Escalation to Supervisor

```rust
// Event received from EventBusActor
Event {
    id: "evt-792",
    event_type: "worker.failed",
    topic: "worker.failed",
    payload: {
        "worker_id": "worker-1",
        "error": "Timeout waiting for response",
    },
    source: "supervisor-1",
    event_depth: 0,
}

// Rule evaluation (count failures in 60-second window)
RuleAst::CountInWindow {
    window_ms: 60_000,
    threshold: 5,
}

// Check failure count in window
let recent_failures = state.events_in_window("worker.failed", 60_000);
if recent_failures >= 5 {
    // Escalate to supervisor
    Alert {
        alert_id: "alert-failure-001",
        rule_id: "rule-worker-failure-spike",
        severity: AlertSeverity::High,
        category: AlertCategory::Observability,
        message: "Worker failure spike: 5 failures in 60s",
        escalation_status: EscalationStatus::Pending,
    };

    ractor::cast!(
        session_supervisor,
        SupervisorMsg::HighSeverityAlert {
            alert: alert.clone(),
        }
    );
}
```

---

## Phase 1 Prototype Scope

### Minimal Viable Prototype

**Goal:** Demonstrate WatcherActor prototype for timeout/failure escalation signals to supervisors (as specified in AGENTS.md).

### In-Scope

1. **Basic Subscription**
   - Topic-based subscription to EventBusActor
   - Single watcher instance
   - 3 example rules (PII detection, slow model call, worker failure)

2. **Rule Evaluation**
   - Deterministic rule evaluation (no ML)
   - Field condition, regex match, count-in-window
   - Single-condition rules only (no AND/OR logic)

3. **Alert Emission**
   - Emit `alert.emitted` event to EventBus
   - 3 severity levels (low, medium, high)
   - 3 categories (security, policy, observability)

4. **Feedback Loop Prevention**
   - Event depth tracking (max depth: 5)
   - Source tagging (`is_watcher_generated: true`)
   - Basic deduplication (60-second window)

5. **Escalation to Supervisor**
   - High/critical alerts escalate to SessionSupervisor
   - Simple escalation message (alert payload)

6. **State Persistence**
   - Persist active subscriptions to EventStore
   - Persist active alerts to EventStore
   - Crash recovery on restart

### Out-of-Scope (Deferred to Phase 2)

- ML-based rule evaluation
- Complex rule logic (AND/OR combinations)
- Pattern-based payload filtering (JSONPath)
- Circuit breaker implementation
- Action blocking (tool execution)
- Logs App integration
- Multi-watcher coordination
- Web UI for rule management

### Success Criteria

1. WatcherActor subscribes to `chat.*`, `model.*`, `worker.*` topics
2. 3 example rules evaluate events correctly
3. Alerts emitted to EventBus for rule matches
4. High/critical alerts escalate to SessionSupervisor
5. Event depth tracking prevents feedback loops
6. State persisted to EventStore and restored on restart
7. Integration test validates end-to-end flow

---

## Appendix: Full WatcherActor Code Structure

```rust
// sandbox/src/actors/watcher.rs

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ractor::{Actor, ActorProcessingErr, ActorRef, RpcReplyPort};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub struct WatcherActor;

pub struct WatcherArguments {
    pub watcher_id: String,
    pub event_bus: ActorRef<EventBusMsg>,
    pub event_store: ActorRef<EventStoreMsg>,
    pub application_supervisor: Option<ActorRef<SupervisorMsg>>,
    pub session_supervisor: Option<ActorRef<SupervisorMsg>>,
    pub config: WatcherConfig,
}

pub struct WatcherState {
    watcher_id: String,
    event_bus: ActorRef<EventBusMsg>,
    event_store: ActorRef<EventStoreMsg>,
    application_supervisor: Option<ActorRef<SupervisorMsg>>,
    session_supervisor: Option<ActorRef<SupervisorMsg>>,
    subscriptions: HashMap<SubscriptionId, ActiveSubscription>,
    rules: HashMap<RuleId, WatchRule>,
    alerts: HashMap<String, Alert>,
    dedup_cache: LruCache<String, DateTime<Utc>>,
    circuit_breaker: CircuitBreaker,
    regex_cache: HashMap<RuleId, Regex>,
    stats: WatcherStats,
    config: WatcherConfig,
}

#[derive(Debug, Clone)]
pub enum WatcherMsg {
    Subscribe { ... },
    Unsubscribe { ... },
    EventReceived { ... },
    GetActiveAlerts { ... },
    AcknowledgeAlert { ... },
    GetStats { ... },
}

#[async_trait]
impl Actor for WatcherActor {
    type Msg = WatcherMsg;
    type State = WatcherState;
    type Arguments = WatcherArguments;

    async fn pre_start(...) -> Result<Self::State, ActorProcessingErr> { ... }
    async fn handle(...) -> Result<(), ActorProcessingErr> { ... }
    async fn post_stop(...) -> Result<(), ActorProcessingErr> { ... }
}

impl WatcherActor {
    async fn handle_subscribe(...) { ... }
    async fn handle_unsubscribe(...) { ... }
    async fn handle_event_received(...) { ... }
    async fn evaluate_rules(...) { ... }
    async fn check_dedup(...) { ... }
    async fn check_circuit_breaker(...) { ... }
    async fn emit_alert(...) { ... }
    async fn escalate_alert(...) { ... }
    async fn persist_state(...) { ... }
    async fn restore_state(...) { ... }
}
```

---

## Conclusion

This research document provides a comprehensive design for WatcherActor architecture in ChoirOS, covering:

- **Subscription API** with topic, pattern, and actor-scoped filtering
- **Rule DSL/AST** with 15 example rules across security, policy, observability
- **Alert classification** with severity levels and escalation paths
- **Feedback loop prevention** via event depth, dedup, circuit breakers, source tagging
- **10 output event types** for alerts, escalations, statistics
- **State persistence strategy** for crash recovery
- **Performance characteristics** targeting 10K events/sec
- **Phase 1 prototype scope** for immediate implementation

The design integrates seamlessly with existing ChoirOS components (EventBusActor, EventStoreActor, supervision tree) and follows ractor actor patterns. Feedback loop prevention mechanisms are comprehensive, addressing the core risk of cascading failures.

**Next Steps:** Implement Phase 1 prototype with 3 example rules, validate escalation to SessionSupervisor, and add integration tests.
