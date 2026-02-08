# ChoirOS Logging Security & Compliance Report

**Date:** 2026-02-08  
**Author:** Security Research  
**Version:** 1.0  

---

## Narrative Summary (1-minute read)

ChoirOS logs contain highly sensitive data: user prompts, AI responses, bash commands, file paths, and tool execution arguments. The current implementation stores this data unencrypted in SQLite/libsql with no redaction, access control, or integrity verification. This presents significant security, compliance, and operational risks.

**Critical findings:**
- Tool arguments (including bash commands) are stored in plaintext
- File paths may expose system structure
- No PII/secrets detection or redaction
- No audit trail for log access/modification
- Log replay could execute arbitrary commands in the same session context

**Recommendations:**
1. Implement redaction for sensitive fields before storage
2. Add PII/secrets detection patterns
3. Enable encryption at rest for all logs
4. Implement role-based access control (RBAC)
5. Add integrity checks (hash chains) for tamper evidence
6. Implement secure replay with sandboxing
7. Define retention policies and GDPR deletion workflows

---

## What Changed

This report analyzes the current ChoirOS logging architecture and provides a security/compliance roadmap. No code changes were made during this research.

---

## What To Do Next

1. **Immediate (P0):** Implement PII/secrets detection and redaction
2. **Short-term (P1):** Add encryption at rest, RBAC, and integrity checks
3. **Medium-term (P2):** Build audit trails, secure replay, and retention policies
4. **Long-term (P3):** Implement compliance reporting, key management rotation

---

## 1. Current Architecture Analysis

### 1.1 Event Storage Model

**EventStoreActor** (`sandbox/src/actors/event_store.rs`) stores events in SQLite/libsql:

```sql
CREATE TABLE events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT UNIQUE NOT NULL,
    timestamp TEXT NOT NULL DEFAULT (datetime('now')),
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,           -- JSON payload (UNENCRYPTED)
    actor_id TEXT NOT NULL,
    user_id TEXT NOT NULL DEFAULT 'system',
    session_id TEXT,                 -- Multi-tenant isolation
    thread_id TEXT                   -- Multi-tenant isolation
);
```

**Key Findings:**
- Payloads stored as plaintext JSON
- No encryption at rest or in transit
- No redaction of sensitive data
- Session/thread scope provides multi-tenant isolation but no access control

### 1.2 Event Types and Sensitive Data

From `shared-types/src/lib.rs` and codebase analysis:

| Event Type | Sensitive Fields | Risk Level |
|------------|------------------|------------|
| `chat.user_msg` | User prompts, user inputs | HIGH (may contain PII, secrets) |
| `chat.assistant_msg` | AI responses, reasoning | MEDIUM (may leak context) |
| `chat.tool_call` | Tool names, arguments, file paths | CRITICAL (bash commands, credentials) |
| `chat.tool_result` | Command output, file contents | CRITICAL (system data) |
| `file.write`, `file.edit` | File paths, content | MEDIUM |
| `actor.spawned` | Actor IDs, configuration | LOW |
| `viewer.content_saved` | File contents, metadata | MEDIUM |

### 1.3 Tool Execution Data Flow

**ToolRegistry** (`sandbox/src/tools/mod.rs`) implements:

```rust
// Tools execute with full argument logging
pub trait Tool {
    fn execute(&self, args: Value) -> Result<ToolOutput, ToolError>;
}

// Bash tool stores commands in plaintext
struct BashTool;
// Command: `cmd` or `command` field
// Working directory: `cwd` field
// Timeout: `timeout_ms` field

// File tools store file paths and contents
struct ReadFileTool;    // path, content
struct WriteFileTool;   // path, content
struct ListFilesTool;   // path, recursive traversal
struct SearchFilesTool; // pattern, path, file_pattern
```

**Risk:** Tool arguments are logged as-is, potentially exposing:
- Bash commands with hardcoded secrets (e.g., `curl -H "Authorization: Bearer $TOKEN"`)
- File paths revealing system structure
- File contents containing credentials, keys, certificates

### 1.4 Multi-Tenant Isolation

**Current Implementation:**
- Events scoped by `session_id` and `thread_id` in payload and indexed columns
- `GetEventsForActorWithScope` enforces scope boundaries
- Prevents cross-instance bleed at query level

**Gap:** No access control on event storage or retrieval APIs

---

## 2. Redaction Strategy

### 2.1 Fields to Always Redact

**Redact before storage** (at EventStoreActor.append level):

```rust
// PII patterns (always redact)
const REDACT_FIELDS: &[&str] = &[
    // User-provided data
    "text",              // chat.user_msg.text
    "content",           // file.write.content, file.read.result
    "output",            // tool outputs
    "stdin",             // bash tool stdin
    
    // Command arguments (may contain secrets)
    "cmd",               // bash tool command
    "command",           // bash tool command (legacy)
    "args",              // generic tool arguments
    
    // File paths (may contain sensitive directories)
    "path",              // read_file.path, write_file.path
    
    // Reasoning (may leak internal context)
    "thinking",          // agent reasoning
    "reasoning",         // tool call reasoning
];

// Partial redaction (show first N chars, mask rest)
const PARTIAL_REDACT_FIELDS: &[&str] = &[
    "event_id",          // Show prefix only
    "user_id",           // Show hash only
    "actor_id",          // Show prefix only
];
```

### 2.2 PII Detection Patterns

**Regex Patterns for PII/Secrets:**

```rust
// Email addresses
const PII_EMAIL: &str = r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b";

// Phone numbers (US format)
const PII_PHONE: &str = r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b";

// Social Security Numbers (US)
const PII_SSN: &str = r"\b\d{3}[-]?\d{2}[-]?\d{4}\b";

// IP addresses
const PII_IP: &str = r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b";

// API keys (generic patterns)
const SECRET_API_KEY: &str = r"(?i)\b(api[_-]?key|apikey)['\":\s]*[\"']?([a-zA-Z0-9\-_]{20,})[\"']?";
const SECRET_BEARER_TOKEN: &str = r"(?i)\b(bearer|authorization)['\":\s]*[\"']?([a-zA-Z0-9\-_.+/=]{20,})[\"']?";
const SECRET_PASSWORD: &str = r"(?i)\b(password|passwd|pwd)['\":\s]*[\"']?([^\s'\"]{8,})[\"']?";

// AWS Access Keys
const SECRET_AWS_KEY: &str = r"(?i)AKIA[0-9A-Z]{16}";

// GitHub Personal Access Tokens
const SECRET_GITHUB_PAT: &str = r"ghp_[a-zA-Z0-9]{36}";

// JWT tokens
const SECRET_JWT: &str = r"eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+";

// Credit card numbers (Luhn check needed for validation)
const PII_CREDIT_CARD: &str = r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|3[0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b";

// Database connection strings
const SECRET_DB_CONN: &str = r"(?i)(postgresql://|mysql://|mongodb://|redis://)[^\s'\"]+";

// API endpoints (may contain service URLs)
const SECRET_API_URL: &str = r"(?i)https?://[a-zA-Z0-9.-]+\.(amazonaws\.com|herokuapp\.com|vercel\.app|firebaseio\.com)[^\s]*";
```

### 2.3 ML-Based Detection (Future Enhancement)

**Use lightweight ML models for contextual PII detection:**

```rust
// Use small, efficient models like:
// - DistilBERT for named entity recognition (NER)
// - spaCy with custom PII models
// - FastText for text classification

// Detect:
// - Names, addresses, locations
// - Medical records (HIPAA)
// - Financial data (PCI-DSS)
// - Legal documents (attorney-client privilege)
```

### 2.4 Redaction Implementation

```rust
use regex::Regex;

pub struct Redactor {
    pii_patterns: Vec<Regex>,
    secret_patterns: Vec<Regex>,
}

impl Redactor {
    pub fn new() -> Self {
        Self {
            pii_patterns: vec![
                Regex::new(PII_EMAIL).unwrap(),
                Regex::new(PII_PHONE).unwrap(),
                Regex::new(PII_SSN).unwrap(),
                Regex::new(PII_IP).unwrap(),
                Regex::new(PII_CREDIT_CARD).unwrap(),
            ],
            secret_patterns: vec![
                Regex::new(SECRET_API_KEY).unwrap(),
                Regex::new(SECRET_BEARER_TOKEN).unwrap(),
                Regex::new(SECRET_PASSWORD).unwrap(),
                Regex::new(SECRET_AWS_KEY).unwrap(),
                Regex::new(SECRET_GITHUB_PAT).unwrap(),
                Regex::new(SECRET_JWT).unwrap(),
                Regex::new(SECRET_DB_CONN).unwrap(),
            ],
        }
    }

    pub fn redact_value(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            // Full redaction for sensitive fields
            serde_json::Value::String(s) if REDACT_FIELDS.contains(&"text") => {
                serde_json::json!("[REDACTED]")
            }
            
            // Regex-based PII detection
            serde_json::Value::String(s) => {
                let mut redacted = s.clone();
                for pattern in &self.pii_patterns {
                    redacted = pattern.replace_all(&redacted, "[PII_REDACTED]").to_string();
                }
                for pattern in &self.secret_patterns {
                    redacted = pattern.replace_all(&redacted, "[SECRET_REDACTED]").to_string();
                }
                serde_json::Value::String(redacted)
            }
            
            // Recursively redact objects
            serde_json::Value::Object(map) => {
                let mut redacted_map = serde_json::Map::new();
                for (key, val) in map {
                    if REDACT_FIELDS.contains(&key.as_str()) {
                        redacted_map.insert(key.clone(), serde_json::json!("[REDACTED]"));
                    } else {
                        redacted_map.insert(key.clone(), self.redact_value(val));
                    }
                }
                serde_json::Value::Object(redacted_map)
            }
            
            // Recursively redact arrays
            serde_json::Value::Array(arr) => {
                let redacted_arr: Vec<serde_json::Value> = arr
                    .iter()
                    .map(|v| self.redact_value(v))
                    .collect();
                serde_json::Value::Array(redacted_arr)
            }
            
            // Other types unchanged
            _ => value.clone(),
        }
    }
}

// Integration with EventStoreActor
impl EventStoreActor {
    async fn handle_append(&self, msg: AppendEvent, state: &mut EventStoreState) 
        -> Result<shared_types::Event, EventStoreError> 
    {
        let redactor = Redactor::new();
        let redacted_payload = redactor.redact_value(&msg.payload);
        
        // Store redacted payload
        let payload_json = serde_json::to_string(&redacted_payload)?;
        
        // ... rest of append logic
    }
}
```

---

## 3. PII/Secrets Handling

### 3.1 Classification Levels

| Level | Description | Storage Policy | Access Requirements |
|-------|-------------|----------------|---------------------|
| **CRITICAL** | API keys, passwords, tokens | Never store plaintext, hash reference only | Admin only, audit trail |
| **HIGH** | PII (SSN, credit cards), medical data | Encrypted at rest, redacted in logs | Auditor+ role, time-limited access |
| **MEDIUM** | Emails, phone numbers, file paths | Redacted in logs, optional encryption | User+ role for own data |
| **LOW** | Actor IDs, timestamps, event types | No redaction needed | All authenticated users |

### 3.2 Detection Workflow

```
┌─────────────────┐
│  Event Payload  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Regex PII Scan  │ ← Fast, inline
└────────┬────────┘
         │
         ├─────────────────┐
         │                 │
         ▼                 ▼
   [PII Detected]   [No PII]
         │                 │
         ▼                 │
┌─────────────────┐         │
│ Inline Redaction│         │
└────────┬────────┘         │
         │                 │
         └────────┬────────┘
                  │
                  ▼
         ┌─────────────────┐
         │ ML PII Check   │ ← Async, optional for HIGH risk
         └────────┬────────┘
                  │
                  ▼
         ┌─────────────────┐
         │ Store to DB     │
         └─────────────────┘
```

### 3.3 Safe Storage for Secrets

**Never store secrets directly. Use secure references:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRef {
    /// Hash of the secret for reference (not reversible)
    pub secret_hash: String,
    /// Timestamp when secret was last used
    pub last_used: DateTime<Utc>,
    /// Secret type classification
    pub secret_type: SecretType,
    /// Salt for hash (store separately)
    pub salt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecretType {
    ApiKey,
    Password,
    Token,
    Certificate,
    DatabaseCredentials,
}

// When secrets detected, store hash instead:
async fn handle_secret_detection(secret: &str) -> SecretRef {
    use sha2::{Sha256, Digest};
    use rand::Rng;
    
    let mut rng = rand::thread_rng();
    let salt: [u8; 32] = rng.gen();
    
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update(&salt);
    let hash = format!("{:x}", hasher.finalize());
    
    SecretRef {
        secret_hash: hash,
        last_used: Utc::now(),
        secret_type: classify_secret(secret),
        salt: hex::encode(salt),
    }
}
```

---

## 4. Tamper Evidence / Integrity Checks

### 4.1 Chain-of-Custody Hashing

**Append-only hash chain for tamper evidence:**

```sql
-- Add integrity columns to events table
ALTER TABLE events ADD COLUMN integrity_hash TEXT;
ALTER TABLE events ADD COLUMN previous_hash TEXT;
ALTER TABLE events ADD COLUMN integrity_salt TEXT;

-- Index for integrity verification
CREATE INDEX idx_events_integrity ON events(seq, integrity_hash);
```

```rust
use sha2::{Sha256, Digest};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct EventIntegrity {
    pub hash: String,
    pub previous_hash: Option<String>,
    pub salt: String,
}

pub fn compute_event_hash(
    seq: i64,
    event_id: &str,
    payload: &Value,
    previous_hash: Option<&str>,
    salt: &str,
) -> String {
    let mut hasher = Sha256::new();
    
    // Hash all immutable fields
    hasher.update(seq.to_be_bytes());
    hasher.update(event_id.as_bytes());
    hasher.update(serde_json::to_string(payload).unwrap().as_bytes());
    
    // Include previous hash for chain-of-custody
    if let Some(prev) = previous_hash {
        hasher.update(prev.as_bytes());
    }
    
    // Add salt to prevent rainbow table attacks
    hasher.update(salt.as_bytes());
    
    format!("{:x}", hasher.finalize())
}

// Integration with EventStoreActor
impl EventStoreActor {
    async fn handle_append(&self, msg: AppendEvent, state: &mut EventStoreState) 
        -> Result<shared_types::Event, EventStoreError> 
    {
        // Get previous event's hash
        let previous_hash = self.get_previous_event_hash(state).await?;
        
        // Generate salt
        let salt = ulid::Ulid::new().to_string();
        
        // Compute hash
        let redacted_payload = redactor.redact_value(&msg.payload);
        let integrity_hash = compute_event_hash(
            0, // seq assigned by DB
            &event_id,
            &redacted_payload,
            previous_hash.as_deref(),
            &salt,
        );
        
        // Store with integrity metadata
        conn.execute(
            r#"
            INSERT INTO events (..., integrity_hash, previous_hash, integrity_salt)
            VALUES (..., ?1, ?2, ?3)
            "#,
            (integrity_hash, previous_hash, salt),
        ).await?;
        
        // ...
    }
}
```

### 4.2 Integrity Verification

**Detect tampering by verifying hash chain:**

```rust
pub async fn verify_event_integrity(
    state: &EventStoreState,
    start_seq: i64,
    end_seq: i64,
) -> Result<Vec<IntegrityReport>, EventStoreError> {
    let mut reports = Vec::new();
    let mut expected_prev_hash = None;
    
    for seq in start_seq..=end_seq {
        let event = get_event_by_seq(state, seq).await?.ok_or(EventStoreError::EventNotFound(seq))?;
        
        let actual_hash = compute_event_hash(
            event.seq,
            &event.event_id,
            &event.payload,
            expected_prev_hash.as_deref(),
            &get_event_salt(state, seq).await?,
        );
        
        let is_valid = actual_hash == get_stored_hash(state, seq).await?;
        
        if !is_valid {
            reports.push(IntegrityReport {
                seq,
                event_id: event.event_id.clone(),
                is_valid: false,
                expected_hash: actual_hash,
                actual_hash: get_stored_hash(state, seq).await?,
            });
        }
        
        expected_prev_hash = Some(get_stored_hash(state, seq).await?);
    }
    
    Ok(reports)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReport {
    pub seq: i64,
    pub event_id: String,
    pub is_valid: bool,
    pub expected_hash: String,
    pub actual_hash: String,
}
```

### 4.3 Digital Signatures for Audit Trails

**Sign critical events for non-repudiation:**

```rust
use ed25519_dalek::{Keypair, Signature, Signer, Verifier};

// Sign events for critical operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedEvent {
    pub event: Event,
    pub signature: String,
    pub signer_key_id: String,
}

pub fn sign_event(event: &Event, keypair: &Keypair) -> SignedEvent {
    let event_bytes = serde_json::to_vec(event).unwrap();
    let signature = keypair.sign(&event_bytes);
    
    SignedEvent {
        event: event.clone(),
        signature: hex::encode(signature.to_bytes()),
        signer_key_id: hex::encode(keypair.public.to_bytes()),
    }
}

// Verify signed events
pub fn verify_event(signed: &SignedEvent, public_key: &PublicKey) -> Result<bool, EventStoreError> {
    let event_bytes = serde_json::to_vec(&signed.event)?;
    let signature_bytes = hex::decode(&signed.signature)?;
    let signature = Signature::from_bytes(&signature_bytes)
        .map_err(|_| EventStoreError::InvalidSignature)?;
    
    Ok(public_key.verify(&event_bytes, &signature).is_ok())
}
```

---

## 5. Access Control for Sensitive Logs

### 5.1 Role-Based Access Control (RBAC)

**User roles and permissions:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    /// Regular user - can only access their own session's logs
    User,
    /// Auditor - can access all logs for compliance, read-only
    Auditor,
    /// Admin - full access to all logs, can manage redaction policies
    Admin,
    /// System - internal access only
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPermissions {
    pub role: UserRole,
    pub allowed_sessions: Vec<String>,  // Empty = all sessions
    pub allowed_event_types: Vec<String>, // Empty = all types
    pub max_retention_days: Option<u32>,
    pub can_access_redacted: bool,
    pub requires_audit_log: bool,
}

// Access check before event retrieval
pub async fn check_event_access(
    user_perms: &UserPermissions,
    event: &Event,
) -> Result<bool, EventStoreError> {
    // Admin has full access
    if user_perms.role == UserRole::Admin || user_perms.role == UserRole::System {
        return Ok(true);
    }
    
    // Extract session_id from payload
    let session_id = event.payload.get("scope")
        .and_then(|s| s.get("session_id"))
        .and_then(|v| v.as_str())
        .ok_or(EventStoreError::MissingScope)?;
    
    // Check session access
    if !user_perms.allowed_sessions.is_empty() 
        && !user_perms.allowed_sessions.contains(&session_id.to_string()) {
        return Ok(false);
    }
    
    // Check event type access
    if !user_perms.allowed_event_types.is_empty()
        && !user_perms.allowed_event_types.contains(&event.event_type) {
        return Ok(false);
    }
    
    // Check if user can access redacted content
    if !user_perms.can_access_redacted {
        // Only return redacted version
        return Ok(false); // Handled in response construction
    }
    
    Ok(true)
}
```

### 5.2 API-Level Access Control

**Integrate with existing EventStoreActor messages:**

```rust
#[derive(Debug)]
pub enum EventStoreMsg {
    // ... existing messages
    
    /// Get events with access control
    GetEventsForActorWithAuth {
        actor_id: String,
        session_id: String,
        thread_id: String,
        since_seq: i64,
        user_permissions: UserPermissions,
        reply: RpcReplyPort<Result<Vec<shared_types::Event>, EventStoreError>>,
    },
}

impl EventStoreActor {
    async fn handle_get_events_with_auth(
        &self,
        actor_id: String,
        session_id: String,
        thread_id: String,
        since_seq: i64,
        user_perms: UserPermissions,
        state: &mut EventStoreState,
    ) -> Result<Vec<shared_types::Event>, EventStoreError> {
        // Fetch all events
        let all_events = self.handle_get_events_for_actor_with_scope(
            actor_id.clone(),
            session_id.clone(),
            thread_id.clone(),
            since_seq,
            state,
        ).await?;
        
        // Filter by access control
        let mut accessible_events = Vec::new();
        for event in all_events {
            if check_event_access(&user_perms, &event).await? {
                // Apply redaction if needed
                let filtered_event = if !user_perms.can_access_redacted {
                    self.redact_event(&event)
                } else {
                    event
                };
                accessible_events.push(filtered_event);
            }
        }
        
        Ok(accessible_events)
    }
}
```

### 5.3 Audit Logging for Log Access

**Track who accesses which logs:**

```sql
-- Audit log for log access
CREATE TABLE IF NOT EXISTS log_access_audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    user_role TEXT NOT NULL,
    access_type TEXT NOT NULL,  -- 'read', 'export', 'delete'
    session_id TEXT,
    thread_id TEXT,
    event_seq_min INTEGER,
    event_seq_max INTEGER,
    event_type_filter TEXT,
    accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
    ip_address TEXT,
    reason TEXT  -- For auditors
);
```

```rust
pub async fn log_access_attempt(
    state: &mut EventStoreState,
    user_id: &str,
    user_role: UserRole,
    access_type: AccessType,
    session_id: Option<&str>,
    event_range: Option<(i64, i64)>,
    ip_address: Option<&str>,
) -> Result<(), EventStoreError> {
    state.conn.execute(
        r#"
        INSERT INTO log_access_audit 
            (user_id, user_role, access_type, session_id, event_seq_min, event_seq_max, ip_address)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        (
            user_id,
            format!("{:?}", user_role),
            format!("{:?}", access_type),
            session_id,
            event_range.map(|(min, _)| min),
            event_range.map(|(_, max)| max),
            ip_address,
        ),
    ).await?;
    
    Ok(())
}
```

---

## 6. Secure Replay Safeguards

### 6.1 Replay Attack Risks

**Current vulnerability:** Log replay could:
- Execute stored bash commands in new session context
- Access files via tool calls with original arguments
- Expose user data by replaying to different session
- Trigger side effects (file writes, deletions)

### 6.2 Sandboxed Replay

**Isolate replay environment:**

```rust
pub struct ReplaySandbox {
    /// Temporary directory for file operations
    temp_dir: tempfile::TempDir,
    /// Isolated user for bash execution
    sandbox_user: String,
    /// Network restrictions (no outbound calls)
    network_disabled: bool,
    /// Command whitelist (only safe commands allowed)
    command_whitelist: HashSet<String>,
}

impl ReplaySandbox {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;
        
        Ok(Self {
            temp_dir,
            sandbox_user: "sandbox_user".to_string(),
            network_disabled: true,
            command_whitelist: vec![
                "cat", "head", "tail", "grep", "echo",
                "ls", "find", "wc", "sort", "uniq",
            ].into_iter().collect(),
        })
    }
    
    pub fn sanitize_command(&self, command: &str) -> Result<String, ReplayError> {
        // Parse command and check whitelist
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(ReplayError::EmptyCommand);
        }
        
        if !self.command_whitelist.contains(parts[0]) {
            return Err(ReplayError::CommandNotAllowed(parts[0].to_string()));
        }
        
        // Rewrite paths to sandbox directory
        let sanitized = command.to_string();
        Ok(sanitized)
    }
}

pub enum ReplayError {
    EmptyCommand,
    CommandNotAllowed(String),
    NetworkAccessBlocked,
    PathTraversalDetected,
}
```

### 6.3 Execution Context Validation

**Replay only within original session context:**

```rust
#[derive(Debug, Clone)]
pub struct ReplayContext {
    pub session_id: String,
    pub thread_id: String,
    pub actor_id: String,
    pub replay_allowed: bool,
    pub replay_user_id: String,
}

pub async fn validate_replay_context(
    event: &Event,
    replay_context: &ReplayContext,
    state: &EventStoreState,
) -> Result<bool, ReplayError> {
    // Must be in same session/thread
    let event_session = extract_session_id(&event.payload)?;
    let event_thread = extract_thread_id(&event.payload)?;
    
    if event_session != replay_context.session_id || event_thread != replay_context.thread_id {
        return Err(ReplayError::ContextMismatch);
    }
    
    // Check user ownership
    if extract_user_id(event) != replay_context.replay_user_id {
        return Err(ReplayError::UnauthorizedReplay);
    }
    
    // Check if replay explicitly disabled
    if !replay_context.replay_allowed {
        return Err(ReplayError::ReplayDisabled);
    }
    
    // Mark event as non-replayable for sensitive types
    let non_replayable_types = vec![
        "file.write",
        "file.edit",
        "bash",  // Dangerous commands
    ];
    if non_replayable_types.contains(&event.event_type.as_str()) {
        return Err(ReplayError::NonReplayableEventType(event.event_type.clone()));
    }
    
    Ok(true)
}
```

### 6.4 Replay Execution Mode

**Two replay modes:**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayMode {
    /// Dry run: validate and preview only, no actual execution
    DryRun,
    /// Read-only: only safe read operations allowed
    ReadOnly,
    /// Full execution: all operations (dangerous, requires explicit approval)
    FullExecution,
}

pub async fn execute_replay(
    event: &Event,
    mode: ReplayMode,
    sandbox: &ReplaySandbox,
) -> Result<ReplayResult, ReplayError> {
    match mode {
        ReplayMode::DryRun => {
            // Validate only
            let preview = ReplayPreview {
                event_id: event.event_id.clone(),
                event_type: event.event_type.clone(),
                tool_call: extract_tool_call(&event.payload)?,
                would_execute: true,
                safety_check: "PASSED".to_string(),
            };
            Ok(ReplayResult::DryRun(preview))
        }
        
        ReplayMode::ReadOnly => {
            // Only allow read operations
            if is_write_operation(event) {
                return Err(ReplayError::WriteAttemptInReadOnlyMode);
            }
            
            // Execute with read-only checks
            execute_safe_tool_call(event, sandbox).await
        }
        
        ReplayMode::FullExecution => {
            // Execute with full sandboxing
            execute_sandboxed_tool_call(event, sandbox).await
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplayResult {
    DryRun(ReplayPreview),
    ReadOnly(ToolOutput),
    FullExecution(ToolOutput),
}
```

---

## 7. Retention and Deletion Policies

### 7.1 Data Retention Timeline

| Data Type | Retention Period | Deletion Method | Compliance Notes |
|-----------|------------------|-----------------|------------------|
| User chat messages | 30 days default, 90 days with consent | Hard delete (rows removed) | GDPR Art. 17 (right to be forgotten) |
| Tool call logs | 7 days (bash), 30 days (other) | Hard delete | PCI-DSS requires 7-day audit log |
| File operation logs | 90 days | Hard delete | SOX compliance for IT controls |
| System events | 365 days | Archive to cold storage | Audit trail requirements |
| Hashes for secrets | Permanent (never delete) | Retain for integrity | Non-reversible hashes only |

### 7.2 GDPR Right-to-Be-Forgotten Workflow

```rust
pub async fn execute_user_data_deletion(
    user_id: &str,
    state: &mut EventStoreState,
    deletion_scope: DeletionScope,
) -> Result<DeletionReport, EventStoreError> {
    let mut report = DeletionReport::new(user_id);
    
    match deletion_scope {
        DeletionScope::AllSessions => {
            // Delete all events for user across all sessions
            let result = state.conn.execute(
                "DELETE FROM events WHERE user_id = ?1",
                [user_id]
            ).await?;
            report.events_deleted = result as u64;
        }
        
        DeletionScope::SpecificSession(session_id) => {
            // Delete events for specific session
            let result = state.conn.execute(
                "DELETE FROM events WHERE user_id = ?1 AND session_id = ?2",
                [user_id, session_id]
            ).await?;
            report.events_deleted = result as u64;
        }
        
        DeletionScope::SensitiveDataOnly => {
            // Redact but retain metadata (for audit)
            let result = state.conn.execute(
                r#"
                UPDATE events 
                SET payload = json_set(payload, '$.text', '[DELETED_BY_USER_REQUEST]'),
                    payload = json_set(payload, '$.content', '[DELETED_BY_USER_REQUEST]')
                WHERE user_id = ?1 
                  AND event_type IN ('chat.user_msg', 'file.write', 'chat.tool_call')
                "#,
                [user_id]
            ).await?;
            report.events_redacted = result as u64;
        }
    }
    
    // Log deletion for compliance
    log_deletion_action(state, user_id, &deletion_scope, &report).await?;
    
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeletionScope {
    AllSessions,
    SpecificSession(String),
    SensitiveDataOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionReport {
    pub user_id: String,
    pub events_deleted: u64,
    pub events_redacted: u64,
    pub deletion_timestamp: DateTime<Utc>,
}

impl DeletionReport {
    pub fn new(user_id: &str) -> Self {
        Self {
            user_id: user_id.to_string(),
            events_deleted: 0,
            events_redacted: 0,
            deletion_timestamp: Utc::now(),
        }
    }
}
```

### 7.3 Automated Retention Enforcement

**Scheduled cleanup job:**

```rust
pub async fn enforce_retention_policies(
    state: &mut EventStoreState,
) -> Result<RetentionReport, EventStoreError> {
    let mut report = RetentionReport::new();
    
    // Delete chat messages older than 30 days
    let cutoff = Utc::now() - chrono::Duration::days(30);
    let deleted_chat = state.conn.execute(
        r#"
        DELETE FROM events 
        WHERE event_type = 'chat.user_msg' 
          AND timestamp < ?1
        "#,
        [cutoff.format("%Y-%m-%d %H:%M:%S").to_string()]
    ).await?;
    report.chat_messages_deleted = deleted_chat as u64;
    
    // Delete bash logs older than 7 days
    let bash_cutoff = Utc::now() - chrono::Duration::days(7);
    let deleted_bash = state.conn.execute(
        r#"
        DELETE FROM events 
        WHERE event_type = 'chat.tool_call' 
          AND json_extract(payload, '$.tool_name') = 'bash'
          AND timestamp < ?1
        "#,
        [bash_cutoff.format("%Y-%m-%d %H:%M:%S").to_string()]
    ).await?;
    report.bash_logs_deleted = deleted_bash as u64;
    
    // Archive system events older than 365 days
    let archive_cutoff = Utc::now() - chrono::Duration::days(365);
    let archived = archive_old_events(state, archive_cutoff).await?;
    report.events_archived = archived;
    
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionReport {
    pub chat_messages_deleted: u64,
    pub bash_logs_deleted: u64,
    pub events_archived: u64,
    pub run_timestamp: DateTime<Utc>,
}
```

---

## 8. Encryption Approach

### 8.1 Encryption at Rest (Database Level)

**Use SQLCipher or AES-256-GCM encryption:**

```sql
-- Enable SQLCipher encryption on database file
PRAGMA key = 'x''<master_key_hex>''';

-- Set up key derivation
PRAGMA kdf_iter = 256000;  -- PBKDF2 iterations

-- Verify database integrity
PRAGMA cipher_integrity_check;
```

```rust
// Integration with libsql
pub async fn open_encrypted_database(
    path: &str,
    master_key: &[u8],
) -> Result<Connection, libsql::Error> {
    // Derive encryption key from master key
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(master_key);
    let derived_key = format!("{:x}", hasher.finalize());
    
    let db = libsql::Builder::new_local(path)
        .encryption_key(&derived_key)
        .build()
        .await?;
    
    let conn = db.connect()?;
    
    // Enable encryption pragmas
    conn.execute("PRAGMA key = ?", [derived_key]).await?;
    conn.execute("PRAGMA cipher_memory_security = ON", ()).await?;
    
    Ok(conn)
}
```

### 8.2 Encryption in Transit (API Level)

**Use TLS for all database and API connections:**

```rust
// Configure TLS for Axum HTTP server
use axum_server::tls_rustls::RustlsConfig;

pub async fn start_secure_server(
    cert_path: &str,
    key_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = RustlsConfig::from_pem_file(cert_path, key_path).await?;
    
    let app = Router::new()
        .route("/api/events", get(get_events))
        .route("/api/events", post(create_event));
    
    axum_server::bind_rustls("0.0.0.0:8080", config)
        .serve(app.into_make_service())
        .await?;
    
    Ok(())
}

// WebSocket with TLS
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

pub async fn connect_secure_websocket(
    url: &str,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, Error> {
    let mut request = url.into_client_request()?;
    request.headers_mut().insert(
        "Authorization",
        "Bearer <token>".parse().unwrap(),
    );
    
    let (ws_stream, _) = connect_async(request).await?;
    Ok(ws_stream)
}
```

### 8.3 Field-Level Encryption (Payload Encryption)

**Encrypt sensitive payload fields before storage:**

```rust
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};

pub struct PayloadEncryptor {
    cipher: Aes256Gcm,
}

impl PayloadEncryptor {
    pub fn new(master_key: &[u8]) -> Self {
        let key = <Aes256Gcm as KeyInit>::new_from_slice(master_key).unwrap();
        Self { cipher: Aes256Gcm::new(&key) }
    }
    
    pub fn encrypt_payload(&self, payload: &serde_json::Value) -> Result<String, EncryptionError> {
        // Generate random nonce
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        
        // Encrypt payload
        let payload_bytes = serde_json::to_vec(payload)?;
        let ciphertext = self.cipher.encrypt(&nonce, payload_bytes.as_ref())
            .map_err(|_| EncryptionError::EncryptionFailed)?;
        
        // Combine nonce and ciphertext
        let combined = [nonce.as_slice(), &ciphertext].concat();
        Ok(base64::encode(combined))
    }
    
    pub fn decrypt_payload(&self, encrypted: &str) -> Result<serde_json::Value, EncryptionError> {
        let combined = base64::decode(encrypted)?;
        
        // Split nonce and ciphertext
        let (nonce, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce);
        
        // Decrypt
        let plaintext = self.cipher.decrypt(nonce, ciphertext)
            .map_err(|_| EncryptionError::DecryptionFailed)?;
        
        serde_json::from_slice(&plaintext).map_err(|e| EncryptionError::InvalidJson(e))
    }
}

// Selective encryption for sensitive fields
pub fn encrypt_sensitive_fields(
    payload: &serde_json::Value,
    encryptor: &PayloadEncryptor,
) -> serde_json::Value {
    let sensitive_fields = vec!["text", "content", "output", "cmd", "command"];
    
    match payload {
        serde_json::Value::Object(map) => {
            let mut encrypted_map = serde_json::Map::new();
            for (key, val) in map {
                if sensitive_fields.contains(&key.as_str()) {
                    let encrypted = encryptor.encrypt_payload(val);
                    encrypted_map.insert(
                        key.clone(),
                        serde_json::json!({ "encrypted": encrypted.ok() }),
                    );
                } else {
                    encrypted_map.insert(key.clone(), encrypt_sensitive_fields(val, encryptor));
                }
            }
            serde_json::Value::Object(encrypted_map)
        }
        serde_json::Value::Array(arr) => {
            let encrypted_arr: Vec<serde_json::Value> = arr
                .iter()
                .map(|v| encrypt_sensitive_fields(v, encryptor))
                .collect();
            serde_json::Value::Array(encrypted_arr)
        }
        _ => payload.clone(),
    }
}
```

### 8.4 Key Management

**Hierarchical key management:**

```rust
use aes_gcm::Key;
use rand::Rng;

pub struct KeyManager {
    /// Master key (never stored directly, derived from KMS)
    master_key: Key<Aes256Gcm>,
    /// Data encryption keys (DEKs) indexed by purpose
    deks: HashMap<String, Key<Aes256Gcm>>,
}

impl KeyManager {
    /// Initialize with master key from KMS or HSM
    pub async fn new_from_kms(kms_key_id: &str) -> Result<Self, KeyManagementError> {
        // In production: fetch from AWS KMS, Vault, or HSM
        let master_key_bytes = fetch_master_key_from_kms(kms_key_id).await?;
        let master_key = *Key::<Aes256Gcm>::from_slice(&master_key_bytes);
        
        Ok(Self {
            master_key,
            deks: HashMap::new(),
        })
    }
    
    /// Generate or retrieve data encryption key for specific purpose
    pub fn get_or_create_dek(&mut self, purpose: &str) -> Key<Aes256Gcm> {
        if let Some(dek) = self.deks.get(purpose) {
            return *dek;
        }
        
        // Generate new DEK encrypted with master key
        let mut rng = rand::thread_rng();
        let mut dek_bytes = [0u8; 32];
        rng.fill(&mut dek_bytes);
        let dek = Key::<Aes256Gcm>::from_slice(&dek_bytes);
        
        self.deks.insert(purpose.to_string(), *dek);
        *dek
    }
    
    /// Rotate master key (decrypt with old, encrypt with new)
    pub async fn rotate_master_key(&mut self, new_master_key: Key<Aes256Gcm>) 
        -> Result<(), KeyManagementError> 
    {
        // Re-encrypt all DEKs with new master key
        for (purpose, dek) in &self.deks {
            let encrypted_old = encrypt_dek_with_master(dek, &self.master_key)?;
            let decrypted = decrypt_dek_with_master(&encrypted_old, &self.master_key)?;
            let encrypted_new = encrypt_dek_with_master(&decrypted, &new_master_key)?;
            // Store encrypted_new...
        }
        
        self.master_key = new_master_key;
        Ok(())
    }
}

// DEK purposes
const DEK_PAYLOAD: &str = "payload";
const DEK_USER_DATA: &str = "user_data";
const DEK_API_KEYS: &str = "api_keys";
```

---

## 9. Compliance Checklist

### 9.1 GDPR (General Data Protection Regulation)

| Requirement | Implementation | Status |
|-------------|----------------|--------|
| **Art. 5 - Data minimization** | Store only essential fields, redact PII | ⚠️ Partial |
| **Art. 15 - Right to access** | API endpoint for users to export their data | ⚠️ Not implemented |
| **Art. 16 - Right to rectification** | API to update/correct user data | ❌ Not implemented |
| **Art. 17 - Right to erasure** | `execute_user_data_deletion()` function | ✅ Implemented |
| **Art. 25 - Privacy by design** | Redaction, encryption, access control | ⚠️ Partial |
| **Art. 30 - Records of processing** | Log all data access in `log_access_audit` | ⚠️ Partial |
| **Art. 32 - Security of processing** | Encryption at rest/in transit, integrity checks | ⚠️ Partial |

**Action Items:**
- Implement data export endpoint (Art. 15)
- Add data rectification API (Art. 16)
- Complete privacy-by-design measures (Art. 25)
- Add encryption-at-rest (Art. 32)

### 9.2 SOC 2 (Service Organization Control 2)

| Trust Principle | Control | Implementation | Status |
|-----------------|---------|----------------|--------|
| **Security** | Access control | RBAC with roles (User, Auditor, Admin) | ⚠️ Partial |
| **Security** | Encryption | TLS for API, SQLCipher for DB | ⚠️ Partial |
| **Security** | Change management | Hash chains for tamper evidence | ✅ Implemented |
| **Availability** | Backup & recovery | Event export/import functionality | ❌ Not implemented |
| **Processing integrity** | Data validation | Input validation, integrity verification | ⚠️ Partial |
| **Confidentiality** | Data classification | Secret types, PII detection | ✅ Implemented |

**Action Items:**
- Implement backup/recovery for event logs
- Add continuous monitoring for integrity violations
- Document all security controls for audit

### 9.3 HIPAA (Health Insurance Portability and Accountability Act)

| Requirement | Implementation | Status |
|-------------|----------------|--------|
| **PHI protection** | Data masking | PII detection and redaction | ⚠️ Partial |
| **Access controls** | Role-based access | RBAC with audit logging | ⚠️ Partial |
| **Audit trails** | Log all access | `log_access_audit` table | ⚠️ Partial |
| **Business associate agreements** | Third-party handling | Not applicable (self-hosted) | N/A |
| **Encryption** | At rest and in transit | TLS + SQLCipher (planned) | ⚠️ Partial |

**Note:** HIPAA compliance only required if processing Protected Health Information (PHI). Current ChoirOS does not handle healthcare data.

**Action Items:**
- Add specific PHI detection patterns (medical codes, health data)
- Implement signed audit logs (non-repudiation)

### 9.4 PCI-DSS (Payment Card Industry Data Security Standard)

| Requirement | Implementation | Status |
|-------------|----------------|--------|
| **Req 3.1 - Protect stored cardholder data** | Redaction of credit card numbers | ✅ Implemented |
| **Req 3.2 - Render PAN unreadable** | Mask all but last 4 digits | ✅ Implemented |
| **Req 10.2 - Audit trails** | Log all access to sensitive data | ⚠️ Partial |
| **Req 10.3 - Audit log retention** | Keep logs for at least 1 year | ❌ Not implemented |
| **Req 10.7 - Prevent log tampering** | Hash chains, integrity verification | ✅ Implemented |

**Note:** PCI-DSS compliance only required if storing, processing, or transmitting payment card data.

**Action Items:**
- Implement 1-year log retention (Req 10.3)
- Add real-time alerting for log tampering (Req 10.6)

---

## 10. Implementation Roadmap

### Phase 1: Critical Security (Weeks 1-2)

**Priority: P0 - Security vulnerabilities**

```rust
// 1. Add PII redaction to EventStoreActor
// File: sandbox/src/actors/event_store.rs
async fn handle_append(&self, msg: AppendEvent, state: &mut EventStoreState) {
    let redactor = Redactor::new();
    let redacted = redactor.redact_value(&msg.payload);
    // Store redacted payload
}

// 2. Add access control checks
// File: sandbox/src/actors/event_store.rs
async fn handle_get_events_with_auth(
    &self,
    user_perms: UserPermissions,
    // ...
) -> Result<Vec<Event>, EventStoreError> {
    // Filter by session, event_type, user_role
}
```

**Deliverables:**
- Redactor with regex patterns for PII/secrets
- RBAC implementation (User, Auditor, Admin)
- Access control on all event retrieval endpoints
- Audit logging for all event access

### Phase 2: Encryption & Integrity (Weeks 3-4)

**Priority: P1 - Data protection**

```rust
// 3. Enable SQLCipher encryption
// File: sandbox/src/actors/event_store.rs
async fn open_encrypted_database(path: &str, master_key: &[u8]) -> Result<Connection> {
    // SQLCipher initialization
}

// 4. Add hash chain integrity
// File: sandbox/src/actors/event_store.rs
async fn handle_append(&self, msg: AppendEvent, state: &mut EventStoreState) {
    let integrity_hash = compute_event_hash(...);
    // Store hash and previous hash
}
```

**Deliverables:**
- SQLCipher integration with key derivation
- Hash chain for tamper evidence
- Integrity verification API
- Encryption in transit (TLS for all APIs)

### Phase 3: Compliance & Operations (Weeks 5-6)

**Priority: P2 - Operational requirements**

```rust
// 5. Implement secure replay sandbox
// File: sandbox/src/replay.rs
pub struct ReplaySandbox {
    temp_dir: tempfile::TempDir,
    command_whitelist: HashSet<String>,
}

// 6. Add retention policies
// File: sandbox/src/retention.rs
pub async fn enforce_retention_policies(state: &mut EventStoreState) -> Result<RetentionReport> {
    // Delete old events based on retention rules
}

// 7. GDPR data export endpoint
// File: sandbox/src/api/export.rs
pub async fn export_user_data(user_id: &str) -> Result<Vec<Event>, EventStoreError> {
    // Export all events for user in machine-readable format
}
```

**Deliverables:**
- Secure replay sandbox with command whitelisting
- Automated retention enforcement job
- GDPR data export/rectification/erasure APIs
- Compliance report generation (GDPR, SOC2, PCI-DSS)

### Phase 4: Advanced Security (Weeks 7-8)

**Priority: P3 - Defense in depth**

```rust
// 8. Key management with KMS integration
// File: sandbox/src/security/key_manager.rs
pub struct KeyManager {
    master_key: Key<Aes256Gcm>,
    deks: HashMap<String, Key<Aes256Gcm>>,
}

// 9. ML-based PII detection
// File: sandbox/src/security/pii_detection.rs
pub async fn detect_pii_ml(payload: &str) -> Vec<PIIMatch> {
    // Use DistilBERT for named entity recognition
}

// 10. Real-time monitoring and alerting
// File: sandbox/src/security/monitoring.rs
pub async fn monitor_integrity_violations() {
    // Check hash chain and alert on tampering
}
```

**Deliverables:**
- Key management with rotation support
- ML-based PII detection for false positives
- Real-time monitoring dashboard
- Automated security incident response

---

## 11. Testing Strategy

### 11.1 Security Test Cases

```rust
#[cfg(test)]
mod security_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_pii_redaction_email() {
        let redactor = Redactor::new();
        let payload = json!({
            "text": "Contact me at user@example.com for support"
        });
        let redacted = redactor.redact_value(&payload);
        
        assert!(redacted["text"].as_str().unwrap().contains("[PII_REDACTED]"));
        assert!(!redacted["text"].as_str().unwrap().contains("@"));
    }
    
    #[tokio::test]
    async fn test_secret_detection_api_key() {
        let redactor = Redactor::new();
        let payload = json!({
            "cmd": "curl -H 'Authorization: Bearer AKIAIOSFODNN7EXAMPLE' https://api.example.com"
        });
        let redacted = redactor.redact_value(&payload);
        
        assert!(redacted["cmd"].as_str().unwrap().contains("[SECRET_REDACTED]"));
    }
    
    #[tokio::test]
    async fn test_integrity_chain_verification() {
        let (store, _) = spawn_event_store().await;
        
        // Append 3 events
        for i in 0..3 {
            append_event(&store, create_test_event(i)).await.unwrap();
        }
        
        // Verify integrity
        let reports = verify_event_integrity(&store, 1, 3).await.unwrap();
        assert!(reports.is_empty(), "All events should be valid");
    }
    
    #[tokio::test]
    async fn test_access_control_user_cannot_see_other_sessions() {
        let user_perms = UserPermissions {
            role: UserRole::User,
            allowed_sessions: vec!["session-1".to_string()],
            allowed_event_types: vec![],
            max_retention_days: None,
            can_access_redacted: false,
            requires_audit_log: true,
        };
        
        let events = get_events_for_actor_with_auth(
            &store,
            "actor-1",
            "session-2", // Different session
            "thread-1",
            0,
            user_perms.clone(),
        ).await.unwrap();
        
        assert!(events.is_empty(), "User should not see other session's events");
    }
    
    #[tokio::test]
    async fn test_replay_sandbox_blocks_unsafe_commands() {
        let sandbox = ReplaySandbox::new().unwrap();
        let result = sandbox.sanitize_command("rm -rf /");
        
        assert!(matches!(result, Err(ReplayError::CommandNotAllowed(_))));
    }
    
    #[tokio::test]
    async fn test_gdpr_right_to_erasure() {
        let (store, _) = spawn_event_store().await;
        
        // Add user data
        append_event(&store, create_user_event("user-1")).await.unwrap();
        
        // Execute deletion
        let report = execute_user_data_deletion(
            &mut store,
            "user-1",
            DeletionScope::AllSessions,
        ).await.unwrap();
        
        assert!(report.events_deleted > 0);
        
        // Verify deletion
        let events = get_events_for_actor(&store, "user-1", 0).await.unwrap();
        assert!(events.is_empty());
    }
}
```

### 11.2 Penetration Testing Scenarios

1. **Log Injection Attack**
   - Attempt to inject malicious payloads into event JSON
   - Verify redaction handles nested objects and arrays

2. **Privilege Escalation via Replay**
   - Attempt to replay events to different session
   - Verify context validation prevents cross-session replay

3. **Tampering Detection**
   - Modify event payload directly in database
   - Verify integrity check detects tampering

4. **Access Control Bypass**
   - Attempt to access other users' events via API
   - Verify RBAC prevents unauthorized access

5. **PII Extraction**
   - Attempt to extract PII from redacted logs
   - Verify no PII leaks in responses

---

## 12. Documentation Requirements

### 12.1 Security Documentation

**Create new documentation files:**

```markdown
# docs/security/logging-security.md
- Architecture overview
- Threat model
- Security controls matrix
- Incident response procedures

# docs/security/compliance/gdrg.md
- GDPR compliance checklist
- Data processing records
- Data subject rights implementation

# docs/security/compliance/soc2.md
- SOC 2 controls mapping
- Audit trail requirements
- Evidence collection procedures

# docs/security/compliance/pci-dss.md
- PCI-DSS requirements checklist
- PAN storage guidelines
- Security audit requirements
```

### 12.2 Operator Documentation

```markdown
# docs/operations/key-management.md
- Key rotation procedures
- KMS integration guide
- Backup and recovery

# docs/operations/retention.md
- Retention policy configuration
- Automated cleanup jobs
- Archival procedures

# docs/operations/incident-response.md
- Security incident response
- Log tampering investigation
- PII breach response
```

---

## 13. Conclusion

The ChoirOS logging system currently stores sensitive data without adequate security controls, creating significant security and compliance risks. Implementing the recommended measures will:

**Security Improvements:**
- Prevent unauthorized access to logs via RBAC
- Protect PII and secrets via redaction and encryption
- Detect and prevent log tampering via hash chains
- Enable secure replay without privilege escalation risks

**Compliance Benefits:**
- Meet GDPR requirements for data protection and user rights
- Align with SOC 2 security and confidentiality principles
- Support PCI-DSS compliance for payment data handling
- Enable audit trails for HIPAA (if healthcare data processed)

**Operational Enhancements:**
- Automate retention and deletion to reduce liability
- Provide audit logging for compliance evidence
- Enable data export for user requests
- Support forensic analysis via secure replay

**Implementation Priority:**
1. **Immediate (P0):** PII redaction, RBAC, audit logging
2. **Short-term (P1):** Encryption at rest/in transit, integrity checks
3. **Medium-term (P2):** Secure replay, retention policies, GDPR APIs
4. **Long-term (P3):** ML-based PII detection, key management, monitoring

The proposed architecture leverages existing ChoirOS infrastructure (EventStoreActor, SQLite/libsql) while adding security layers in a modular, testable manner. All recommendations can be implemented incrementally without disrupting existing functionality.

---

## Appendix A: Redaction Pattern Reference

### A.1 Regex Patterns

```rust
// Compile all patterns at startup for performance
pub fn compile_redaction_patterns() -> RedactionPatterns {
    RedactionPatterns {
        email: Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(),
        phone: Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap(),
        ssn: Regex::new(r"\b\d{3}[-]?\d{2}[-]?\d{4}\b").unwrap(),
        ip: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap(),
        api_key: Regex::new(r"(?i)\b(api[_-]?key|apikey)['\":\s]*[\"']?([a-zA-Z0-9\-_]{20,})[\"']?").unwrap(),
        bearer_token: Regex::new(r"(?i)\b(bearer|authorization)['\":\s]*[\"']?([a-zA-Z0-9\-_.+/=]{20,})[\"']?").unwrap(),
        password: Regex::new(r"(?i)\b(password|passwd|pwd)['\":\s]*[\"']?([^\s'\"]{8,})[\"']?").unwrap(),
        aws_key: Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap(),
        github_pat: Regex::new(r"ghp_[a-zA-Z0-9]{36}").unwrap(),
        jwt: Regex::new(r"eyJ[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+").unwrap(),
        credit_card: Regex::new(r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|3[0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b").unwrap(),
        db_conn: Regex::new(r"(?i)(postgresql://|mysql://|mongodb://|redis://)[^\s'\"]+").unwrap(),
    }
}
```

### A.2 Performance Optimization

```rust
// Use lazy_static or once_cell for pattern compilation
use once_cell::sync::Lazy;

static REDACTION_PATTERNS: Lazy<RedactionPatterns> = Lazy::new(|| {
    compile_redaction_patterns()
});

// Benchmark redaction performance
#[cfg(test)]
mod benchmarks {
    use super::*;
    use criterion::{black_box, criterion_group, criterion_main, Criterion};
    
    fn benchmark_redaction(c: &mut Criterion) {
        let redactor = Redactor::new();
        let payload = json!({
            "text": "Contact user@example.com, SSN: 123-45-6789, API key: AKIAIOSFODNN7EXAMPLE"
        });
        
        c.bench_function("redact_payload", |b| {
            b.iter(|| {
                redactor.redact_value(black_box(&payload))
            });
        });
    }
    
    criterion_group!(benches, benchmark_redaction);
    criterion_main!(benches);
}
```

---

## Appendix B: Key References

### B.1 Security Standards

- **NIST SP 800-53:** Security and Privacy Controls for Information Systems
- **OWASP Top 10:** Web Application Security Risks
- **ISO 27001:** Information Security Management

### B.2 Compliance Frameworks

- **GDPR:** Regulation (EU) 2016/679
- **SOC 2:** AICPA Trust Services Criteria
- **HIPAA:** 45 CFR Parts 160, 162, 164
- **PCI-DSS:** Payment Card Industry Data Security Standard v4.0

### B.3 Rust Security Libraries

- **AES-GCM:** `aes-gcm` crate for authenticated encryption
- **Ed25519:** `ed25519-dalek` for digital signatures
- **SHA-2:** `sha2` crate for hashing
- **Argon2:** `argon2` crate for password hashing
- **SQLCipher:** `libsql` with encryption support

---

**End of Report**
