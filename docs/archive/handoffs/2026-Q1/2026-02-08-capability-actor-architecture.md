# Capability Actor Architecture

**Date:** 2026-02-08
**Status:** Design Document
**Priority:** Foundation for Phase C

---

## Core Principle

> Docs first, tests second, code third.

All capability actors share one contract. Safety is not a separate system—it is a capability actor that can block, approve, or escalate.

---

## The Capability Contract

Every tool-like thing implements one interface:

```rust
pub trait CapabilityActor: Actor {
    async fn plan(&self, input: CapabilityInput) -> Result<Plan, ActorError>;
    async fn execute(&self, plan: Plan) -> Result<ExecutionStream, ActorError>;
    async fn stream_update(&self) -> Result<Update, ActorError>;
    async fn complete(&self, result: CapabilityOutput) -> Result<Receipt, ActorError>;
    async fn fail(&self, error: ActorError) -> Result<Receipt, ActorError>;
}
```

### Standard Envelope

All capability calls use the same envelope:

```rust
pub struct CapabilityInput {
    pub task_id: TaskId,
    pub correlation_id: CorrelationId,
    pub scope: Scope,                    // session_id, thread_id
    pub capability: CapabilityId,        // "git.commit", "mcp.tool_name", "human.confirm"
    pub input: serde_json::Value,
    pub compressed_state: CompressedStateSnapshot,  // From StateIndexActor
    pub safety_policy: SafetyPolicy,     // Requirements for this call
}

pub struct CapabilityOutput {
    pub task_id: TaskId,
    pub status: CompletionStatus,        // success, failed, blocked, escalated
    pub result: serde_json::Value,
    pub events: Vec<CapabilityEvent>,    // Audit trail
    pub receipt: Receipt,                // Cryptographic proof of execution
}
```

### Event Schema (Observability)

All capability actors emit the same event types:

```rust
pub enum CapabilityEvent {
    // Lifecycle
    TaskAccepted { task_id, timestamp, actor_id },
    TaskProgress { task_id, percent, message, timestamp },

    // Recursive capability calls
    TaskToolCall { task_id, capability, args, correlation_id },
    TaskToolResult { task_id, capability, result, latency_ms },

    // Safety checkpoints
    TaskSafetyCheck { task_id, check_type, passed, details },
    TaskBlocked { task_id, reason, requires_human },
    TaskEscalated { task_id, from_actor, to_actor, reason },

    // Completion
    TaskComplete { task_id, output_receipt },
    TaskFailed { task_id, error, retryable, compensation_action },
}
```

---

## Actor Hierarchy

```
PromptBarActor                    // Universal entrypoint #1: typed intent
├── IntentRouter                  // NL → structured capability calls
│   ├── ChatActorSpawner
│   ├── TerminalActorSpawner
│   ├── GitActorSpawner
│   ├── MCPActorSpawner
│   └── AppActorSpawner
│
MailAppActor                      // Universal entrypoint #2: email ingress
├── MailRouter                    // Parses email → structured intent
│   ├── Same routing as PromptBar
│   └── HumanResponseHandler      // Replies to HITL requests
│
StateIndexActor                   // AHDB: compressed state plane
├── EventLogReader
├── WatcherSignalProcessor
└── SnapshotGenerator
│
SafetyOrchestratorActor           // Safety capability coordinator
├── VerifierActorPool             // Automatic verification
├── HumanInTheLoopActor           // Email-based confirmation
└── PolicyEnforcementActor        // Static rule checking
│
Capability Pools
├── GitActorPool                  // Per-repo git actors
├── MCPActorPool                  // Per-server MCP actors
└── TerminalActorPool             // Per-session terminal actors
│
SandboxSpawnerActor (future)      // Choir calls Choir
├── LocalSandboxSpawner
└── RemoteSandboxSpawner
```

---

## Safety Actors

Safety is not a separate layer—it is a capability that other capabilities depend on.

### 1. VerifierActor (Automatic)

```rust
pub struct VerifierActor {
    verification_policy: VerificationPolicy,
    llm_client: BamlClient,
}

pub enum VerifierMsg {
    VerifyCode {
        code: String,
        context: CodeContext,
        checks: Vec<VerificationCheck>,  // syntax, security, style
        reply: RpcReplyPort<VerificationResult>,
    },
    VerifyClaim {
        claim: String,
        sources: Vec<Source>,
        reply: RpcReplyPort<ClaimVerification>,
    },
}
```

### 2. HumanInTheLoopActor (Email-Based)

```rust
pub struct HumanInTheLoopActor {
    resend_client: ResendClient,      // From .env RESEND_API_KEY
    mymx_client: MymxClient,          // Agent email platform
    pending_confirmations: HashMap<ConfirmationId, PendingConfirmation>,
    timeout_policy: TimeoutPolicy,    // Default: 24 hours
}

pub enum HITLMsg {
    RequestConfirmation {
        request: ConfirmationRequest,   // What needs approval
        urgency: Urgency,               // low, medium, high, critical
        timeout: Option<Duration>,
        reply: RpcReplyPort<ConfirmationId>,
    },

    // Called when human responds via email
    ProcessEmailReply {
        email: IncomingEmail,
        reply: RpcReplyPort<Result<ConfirmationResult, HITLError>>,
    },

    // Check status
    GetConfirmationStatus {
        confirmation_id: ConfirmationId,
        reply: RpcReplyPort<ConfirmationStatus>,
    },
}

pub struct ConfirmationRequest {
    pub confirmation_id: ConfirmationId,
    pub title: String,                  // Email subject
    pub description: String,            // Email body (HTML/markdown)
    pub proposed_action: CapabilityCall, // What will happen if approved
    pub context: serde_json::Value,     // Full context for review
    pub approve_url: Url,               // One-click approve link
    pub deny_url: Url,                  // One-click deny link
    pub reprompt_url: Url,              // Link to provide new instructions
    pub expires_at: DateTime<Utc>,
}
```

**Email Flow:**

```
1. Capability actor calls SafetyOrchestrator
2. SafetyOrchestrator decides HITL is required
3. HITLActor generates confirmation request
4. HITLActor sends email via Resend:

   Subject: [ChoirOS] Approval required: Deploy to production

   A capability actor (GitActor) wants to:
   → Push commits to main branch
   → Trigger deployment pipeline

   Context:
   - 3 commits by VerificationActor (passed)
   - Safety score: 0.94
   - Estimated impact: Production

   [Approve] [Deny] [Modify Request]

   This request expires in 24 hours.
   Reply to this email with questions.

5. Human clicks link or replies
6. HITLActor processes response
7. Original capability call resumes or fails
```

### 3. PolicyEnforcementActor (Static Rules)

```rust
pub struct PolicyEnforcementActor {
    policies: Vec<SafetyPolicy>,
}

pub enum PolicyMsg {
    CheckPolicy {
        capability_call: CapabilityCall,
        context: ExecutionContext,
        reply: RpcReplyPort<PolicyResult>,
    },
}

pub enum PolicyResult {
    Allow,                              // Proceed
    Block { reason: String },          // Hard stop
    RequireVerification { level: VerificationLevel },
    RequireHumanApproval { urgency: Urgency },
}
```

---

## Safety Decision Flow

```
Capability Call
      │
      ▼
PolicyEnforcementActor (static rules)
      │
      ├─→ Block ───────────→ Fail
      │
      ├─→ Allow ───────────→ Execute
      │
      ├─→ RequireVerification ─→ VerifierActor ─┬─→ Pass ─→ Execute
      │                                          └─→ Fail ─→ Fail/Escalate
      │
      └─→ RequireHumanApproval ─→ HITLActor ─┬─→ Approve ─→ Execute
                                             ├─→ Deny ─────→ Fail
                                             └─→ Timeout ──→ Escalate
```

---

## Entrypoints

### Entrypoint 1: PromptBarActor

**Purpose:** Universal typed intent
**Input:** Natural language, keyboard, voice
**Output:** Structured capability call

```rust
pub enum PromptBarMsg {
    // Direct capability invocation
    Intent {
        natural_language: String,
        current_context: Context,
        reply: RpcReplyPort<CapabilityCall>,
    },

    // Open specific apps
    OpenChat { prompt: String, context: Option<ChatContext> },
    OpenTerminal { command: String, cwd: Option<PathBuf> },
    OpenMail { compose_to: Option<EmailAddress> },

    // Meta
    CreateApp { app_type: AppType, initial_state: JsonValue },
    SpawnCapability { capability: CapabilityId, input: JsonValue },
}
```

### Entrypoint 2: MailAppActor

**Purpose:** Email-based ingress and HITL responses
**Input:** Incoming email (via mymx + Resend)
**Output:** Structured intent or confirmation response

```rust
pub struct MailAppActor {
    mymx_inbox: MymxInbox,            // Agent email address
    resend: ResendClient,
    parser: EmailIntentParser,        // LLM-based parser
}

pub enum MailAppMsg {
    // Incoming email from mymx webhook
    ReceiveEmail {
        email: IncomingEmail,
        reply: RpcReplyPort<EmailHandlingResult>,
    },

    // HITL confirmation response
    ProcessConfirmationReply {
        confirmation_id: ConfirmationId,
        email: IncomingEmail,
        reply: RpcReplyPort<ConfirmationResult>,
    },

    // Send outgoing mail
    SendEmail {
        to: EmailAddress,
        subject: String,
        body: EmailBody,
        reply: RpcReplyPort<Result<EmailReceipt, EmailError>>,
    },
}
```

**Email Intent Parsing:**

```rust
pub enum ParsedEmailIntent {
    // User sends task via email
    CapabilityRequest {
        capability: CapabilityId,
        input: JsonValue,
        requested_by: EmailAddress,
    },

    // Reply to HITL request
    ConfirmationResponse {
        confirmation_id: ConfirmationId,
        decision: ConfirmationDecision,  // approve, deny, modify
        comment: Option<String>,
    },

    // Question/Clarification
    ClarificationRequest {
        refers_to: Option<CorrelationId>,
        question: String,
    },

    // Unparseable
    Unknown { raw_content: String },
}
```

---

## Implementation Order

### Phase 1: Schema & Contracts (Docs)

1. `shared-types/src/capability.rs` - Core types
2. `shared-types/src/safety.rs` - Safety policy types
3. `shared-types/src/hitl.rs` - Human-in-the-loop types
4. `docs/design/capability-lifecycle.md` - State machine docs
5. `docs/design/safety-decision-flow.md` - Safety architecture docs

### Phase 2: State Plane (Code)

1. `StateIndexActor` - Compressed state snapshots
2. Event ingestion pipeline
3. Snapshot query API

### Phase 3: First Capability (Code)

1. `PromptBarActor` - Intent routing
2. `GitActor` - First concrete capability
3. Safety integration (Policy + Verifier)

### Phase 4: HITL (Code)

1. `HumanInTheLoopActor`
2. Resend integration (from .env)
3. mymx configuration
4. `MailAppActor`

### Phase 5: MCP (Code)

1. `MCPActor` with mcporter
2. Tool routing
3. Recursive capability calls

---

## Configuration

### Environment Variables

```bash
# Resend (email delivery)
RESEND_API_KEY=re_xxxxxxxx
RESEND_FROM_EMAIL=choir@yourdomain.com

# mymx (agent email ingress)
MYMX_API_KEY=mx_xxxxxxxx
MYMX_AGENT_EMAIL=agent-xxxxx@mymx.io
MYMX_WEBHOOK_URL=https://yourchoir.instance/webhooks/mymx

# Safety defaults
HITL_DEFAULT_TIMEOUT_HOURS=24
HITL_CRITICAL_TIMEOUT_MINUTES=15
SAFETY_DEFAULT_POLICY=balanced  # permissive, balanced, strict
```

---

## Testing Strategy

1. **Unit:** Each capability actor in isolation
2. **Integration:** Capability → Safety → Execution flow
3. **E2E:** Email → MailApp → HITL → GitActor flow
4. **Fuzz:** Random capability calls with safety policies

---

## Open Questions

1. **HITL timeouts:** What happens when human doesn't respond?
2. **Email threading:** How to correlate email replies with original requests?
3. **Mymx vs Resend:** Do we need both, or can Resend handle inbound?
4. **Safety recursion:** Can a safety actor trigger another safety check?
5. **Receipt cryptography:** Do we need signed receipts for audit?

---

## Next Actions

- [ ] Review this document
- [ ] Approve schema in `shared-types/src/capability.rs`
- [ ] Write `docs/design/capability-lifecycle.md` state machine
- [ ] Create tickets for Phase 1 (schema docs)
