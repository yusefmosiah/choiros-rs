# Refactor Checklist: Eliminate Ad Hoc Workflow Logic

**Date:** 2026-02-09
**Status:** Draft - Ready for Review
**Owner:** Systems Architecture Team
**Related:** RLM_INTEGRATION_REPORT.md, directives-execution-checklist.md, state_index_addendum.md

---

## 1. Narrative Summary (1-Minute Read)

The ChoirOS codebase currently relies on brittle natural-language string matching to control critical workflow paths. This "ad hoc workflow" anti-pattern appears in three major categories:

1. **Stale Status Detection** - Matching phrases like "I'm searching" or "still running" to detect incomplete responses
2. **Tool Dump Detection** - Matching prefixes like "Research results for '" to detect raw tool output
3. **Failure Classification** - Matching error text like "timeout" or "could not resolve host" to categorize failures

These string-based control flows are fragile, model-dependent, and create hidden coupling between prompt engineering and runtime behavior. A model retraining or prompt tweak can break system correctness.

This refactor replaces all string-based control flow with typed protocol fields:
- `ObjectiveStatus` enum: `satisfied | in_progress | blocked`
- `PlanMode` enum: `call_tools | finalize | escalate`
- `FailureKind` enum: `timeout | network | auth | rate_limit | validation`
- Structured `ObjectiveContract` for parent-child delegation

The goal is **deterministic, type-safe orchestration** where control flow depends on protocol fields, not natural language heuristics.

---

## 2. What Changed (Proposed Architecture)

### 2.1 Core Type System

#### ObjectiveStatus (Replace string comparisons)
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveStatus {
    Satisfied,     // Objective complete, final_response required
    InProgress,    // Still working, tool_calls allowed
    Blocked,       // Cannot proceed, completion_reason required
}
```

**Replaces:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:351-367` - String-based status checking
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:1773-1777` - Delegated outcome status parsing

#### PlanMode (Explicit planning state)
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanMode {
    CallTools,     // Execute tool calls
    Finalize,      // Synthesize final response
    Escalate,      // Escalate to parent/supervisor
}
```

#### FailureKind (Typed error taxonomy)
```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Timeout,       // Time limit exceeded
    Network,       // Connectivity issues
    Auth,          // Authentication/authorization failed
    RateLimit,     // Rate limit hit
    Validation,    // Input validation failed
    Provider,      // Upstream provider error
    Unknown,       // Unclassified failure
}
```

**Replaces:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/watcher.rs:378-395` - String-based timeout detection
- `/Users/wiz/choiros-rs/sandbox/src/actors/watcher.rs:416-444` - String-based network failure detection
- `/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs:2283` - String-based timeout matching

### 2.2 ObjectiveContract (Parent-Child Delegation)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveContract {
    pub objective_id: String,              // Unique objective identifier
    pub parent_objective_id: Option<String>, // Hierarchy linkage
    pub primary_objective: String,         // What to accomplish
    pub success_criteria: Vec<String>,     // Measurable completion criteria
    pub constraints: ObjectiveConstraints, // Budgets, timeouts, policies
    pub attempts_budget: u8,               // Max retry attempts
    pub evidence_requirements: EvidenceRequirements, // What evidence to collect
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveConstraints {
    pub max_tool_calls: u32,
    pub timeout_ms: u64,
    pub max_subframe_depth: u8,
    pub allowed_capabilities: Vec<String>, // Capability whitelist
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRequirements {
    pub requires_citations: bool,
    pub min_confidence: f64,
    pub required_source_types: Vec<String>,
}
```

**Replaces:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:214-219` - String-based objective contract construction
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:221-227` - String parsing of objective contracts

### 2.3 CompletionPayload (Child-to-Parent Reporting)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionPayload {
    pub objective_status: ObjectiveStatus,
    pub objective_fulfilled: bool,         // Explicit completion boolean
    pub completion_reason: String,         // Why completed/blocked
    pub evidence: Vec<Evidence>,           // Structured evidence
    pub unresolved_items: Vec<UnresolvedItem>, // What remains undone
    pub recommended_next_action: Option<NextAction>, // Suggested continuation
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub evidence_id: String,
    pub evidence_type: EvidenceType,
    pub source: String,
    pub content: String,
    pub confidence: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextAction {
    pub action_type: NextActionType,       // escalate | continue | complete
    pub recommended_capability: Option<String>,
    pub recommended_objective: Option<String>,
    pub rationale: String,
}
```

**Replaces:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:229-270` - JSON value parsing of delegated outcomes
- `/Users/wiz/choiros-rs/shared-types/src/lib.rs:350-360` - String-based DelegatedTaskOutcome

### 2.4 BAML Schema Updates

Update `/Users/wiz/choiros-rs/baml_src/types.baml`:

```baml
enum ObjectiveStatus {
  satisfied
  in_progress
  blocked
}

enum PlanMode {
  call_tools
  finalize
  escalate
}

class ObjectiveContract {
  objective_id string
  parent_objective_id string?
  primary_objective string
  success_criteria string[]
  max_tool_calls int
  timeout_ms int
  attempts_budget int
}

class CompletionPayload {
  objective_status ObjectiveStatus
  objective_fulfilled bool
  completion_reason string
  evidence Evidence[]
  unresolved_items UnresolvedItem[]
  recommended_next_action NextAction?
}

class AgentPlan {
  thinking string
  tool_calls AgentToolCall[]
  final_response string?
  objective_status ObjectiveStatus  // Changed from string
  plan_mode PlanMode                // NEW: Explicit mode
  completion_reason string?
  confidence float
}
```

**Critical:** The current BAML source is embedded in `/Users/wiz/choiros-rs/sandbox/src/baml_client/baml_source_map.rs`. Both the `.baml` source files AND the embedded source map must be kept in sync.

---

## 3. Audit Findings: String-Matching Control Paths

### 3.1 CRITICAL: Must Remove (Protocol Control)

| File | Line(s) | Pattern | Risk |
|------|---------|---------|------|
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 193-206 | `looks_like_stale_status()` - Matches "search is currently running", "I'm searching", "still running", "in the background" | Controls replanning logic; model-specific phrasing |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 272-311 | `intent_alignment_gap()` - Uses `looks_like_stale_status()`, matches "would you like me to search", "could you clarify" | Gates final response emission |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 351-367 | `objective_status_is_satisfied/blocked/in_progress()` - String comparison against "satisfied", "blocked", "in_progress" | Core state machine logic on untyped strings |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 494-504 | `looks_like_tool_dump_final_response()` - Matches "research results for '", "via tavily:", "delegated objective feedback" | Controls response quality gates |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 528-545 | `answer_directly_addresses_prompt()` - Uses `looks_like_stale_status()`, keyword overlap | Determines answer validity |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 1773-1777 | Delegated outcome status parsing with `eq_ignore_ascii_case("complete")` | Completion detection |
| `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs` | 313-404 | `assess_objective_completion()` - Uses token coverage heuristics, not typed quality signals | Research completion logic |
| `/Users/wiz/choiros-rs/sandbox/src/actors/watcher.rs` | 378-395 | `is_timeout_failure()` - Matches "timeout", "timed out", "deadline", "did not return within" | Failure classification |
| `/Users/wiz/choiros-rs/sandbox/src/actors/watcher.rs` | 397-414 | `is_retry_progress()` - Matches "retry" in phase/status/message | Progress classification |
| `/Users/wiz/choiros-rs/sandbox/src/actors/watcher.rs` | 416-444 | `is_network_failure()` - Matches "could not resolve host", "couldn't connect", "connection reset", "network" | Failure classification |
| `/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs` | 2283 | `lower_summary.contains("timed out") \|\| lower_summary.contains("deadline")` | Timeout detection |
| `/Users/wiz/choiros-rs/sandbox/tests/chat_superbowl_live_matrix_test.rs` | 312-340 | `looks_like_final_answer_for_prompt()` - Matches uncertainty phrases, tool dump patterns | Test quality assertions |
| `/Users/wiz/choiros-rs/sandbox/tests/chat_superbowl_live_matrix_test.rs` | 627-640 | `polluted_followup` detection - Matches "running in the background", "I'm searching" | Test pollution detection |

### 3.2 BENIGN: May Keep (Input Normalization)

| File | Line(s) | Pattern | Justification |
|------|---------|---------|---------------|
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 181-191 | `research_delegate_async_by_default()` - Parses "sync" / "async" from env | Environment config parsing, not control flow |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 1352-1360 | `model_context_trace_enabled()` - Parses "0", "false", "off" | Config parsing |
| `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs` | 293-311 | `relevance_tokens()` - Stopword filtering for search relevance | Information retrieval preprocessing |
| `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs` | 452-458 | `parse_provider_token()` - Matches "tavily", "brave", "exa" | External API provider selection (input normalization) |
| `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs` | 508-517 | `map_time_range_to_brave()` - Maps "day"/"week"/"month" to API codes | External API parameter translation |
| `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs` | 1007-1022 | Command safety checks - matches "ls ", "cat ", "curl " | Input validation for terminal commands |

### 3.3 REQUIRES ANALYSIS

| File | Line(s) | Pattern | Concern |
|------|---------|---------|---------|
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 319-340 | `needs_verifiable_evidence()` - Keyword list for evidence detection | May be acceptable as intent classification, but could become typed |
| `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` | 506-525 | `normalized_keywords()` - Stopword removal for keyword matching | Used for answer quality; consider typed relevance signals |

---

## 4. What To Do Next (Phased Checklist)

### Phase 1: Type Foundation (Week 1)
**Goal:** Define and land all core types without breaking existing functionality.

| Task | Owner | File(s) | Dependencies | Acceptance Criteria |
|------|-------|---------|--------------|---------------------|
| 1.1 Define ObjectiveStatus enum | Architecture | `shared-types/src/lib.rs` | None | Enum serializes/deserializes correctly; matches BAML enum |
| 1.2 Define PlanMode enum | Architecture | `shared-types/src/lib.rs` | None | Enum serializes/deserializes correctly |
| 1.3 Define FailureKind enum | Architecture | `shared-types/src/lib.rs` | None | Enum with all taxonomy variants |
| 1.4 Define ObjectiveContract struct | Architecture | `shared-types/src/lib.rs` | 1.1 | All fields present; JSON round-trip works |
| 1.5 Define CompletionPayload struct | Architecture | `shared-types/src/lib.rs` | 1.1, 1.4 | All fields present; JSON round-trip works |
| 1.6 Update BAML types.baml | Architecture | `baml_src/types.baml` | 1.1-1.5 | BAML enums/classes match Rust types |
| 1.7 Update BAML agent.baml prompts | Architecture | `baml_src/agent.baml` | 1.6 | Prompts reference typed enums, not strings |
| 1.8 Regenerate BAML client | Architecture | `sandbox/src/baml_client/` | 1.6-1.7 | `cargo build` succeeds; types match |
| 1.9 Add CI check for BAML sync | DevOps | `.github/workflows/` | 1.8 | CI fails if `baml_src/` and `baml_source_map.rs` diverge |

**Risk Register:**
- BAML regeneration may produce breaking API changes - Mitigation: Pin BAML version; test in branch
- Type mismatches between BAML and Rust - Mitigation: Automated type comparison test

### Phase 2: Critical Path Refactor (Week 2)
**Goal:** Replace string-based status checking in chat_agent.rs

| Task | Owner | File(s) | Dependencies | Acceptance Criteria |
|------|-------|---------|--------------|---------------------|
| 2.1 Replace objective_status string checks | Backend | `sandbox/src/actors/chat_agent.rs:351-427` | 1.1 | Uses ObjectiveStatus enum; all `eq_ignore_ascii_case` removed |
| 2.2 Replace looks_like_stale_status() | Backend | `sandbox/src/actors/chat_agent.rs:193-206` | 1.2 | Uses typed PlanMode or objective_status from BAML output |
| 2.3 Replace looks_like_tool_dump_final_response() | Backend | `sandbox/src/actors/chat_agent.rs:494-504` | 1.4 | Uses structured evidence fields from CompletionPayload |
| 2.4 Update intent_alignment_gap() | Backend | `sandbox/src/actors/chat_agent.rs:272-311` | 2.1-2.3 | Uses typed status checks, not string matching |
| 2.5 Update answer_directly_addresses_prompt() | Backend | `sandbox/src/actors/chat_agent.rs:528-545` | 2.4 | Uses typed relevance signals or removes heuristic |
| 2.6 Update delegated outcome parsing | Backend | `sandbox/src/actors/chat_agent.rs:229-270` | 1.5 | Parses into CompletionPayload struct |
| 2.7 Unit tests for typed status logic | Backend | `sandbox/src/actors/chat_agent.rs` (tests) | 2.1-2.6 | All status transitions tested; no string matching in tests |

**Risk Register:**
- Behavioral changes in edge cases - Mitigation: Comprehensive test matrix before/after
- Model compatibility - Mitigation: A/B test with old/new prompt formats

### Phase 3: Failure Classification (Week 3)
**Goal:** Replace string-based failure detection with typed FailureKind

| Task | Owner | File(s) | Dependencies | Acceptance Criteria |
|------|-------|---------|--------------|---------------------|
| 3.1 Add FailureKind to worker events | Backend | `shared-types/src/lib.rs` | 1.3 | Worker failure events include typed FailureKind |
| 3.2 Update Watcher timeout detection | Backend | `sandbox/src/actors/watcher.rs:378-395` | 3.1 | Uses FailureKind::Timeout from events, not string matching |
| 3.3 Update Watcher network detection | Backend | `sandbox/src/actors/watcher.rs:416-444` | 3.1 | Uses FailureKind::Network from events, not string matching |
| 3.4 Update Supervisor timeout detection | Backend | `sandbox/src/supervisor/mod.rs:2283` | 3.1 | Uses typed failure classification |
| 3.5 Propagate FailureKind from tools | Backend | `sandbox/src/tools/mod.rs` | 3.1 | Tool errors include FailureKind classification |
| 3.6 Update TerminalActor error mapping | Backend | `sandbox/src/actors/terminal.rs` | 3.5 | Maps terminal errors to FailureKind |
| 3.7 Update ResearcherActor error mapping | Backend | `sandbox/src/actors/researcher.rs` | 3.5 | Maps provider errors to FailureKind |

**Risk Register:**
- External API error format changes - Mitigation: Keep string matching as fallback with telemetry

### Phase 4: Researcher Quality Signals (Week 4)
**Goal:** Replace lexical citation coverage with typed quality output

| Task | Owner | File(s) | Dependencies | Acceptance Criteria |
|------|-------|---------|--------------|---------------------|
| 4.1 Define ResearchQuality struct | Architecture | `shared-types/src/lib.rs` | 1.1 | Contains coverage_score, confidence, source_quality |
| 4.2 Update ResearcherResult | Architecture | `sandbox/src/actors/researcher.rs:97-112` | 4.1 | Includes ResearchQuality field |
| 4.3 Replace assess_objective_completion() | Backend | `sandbox/src/actors/researcher.rs:313-404` | 4.2 | Returns typed ResearchQuality, not just status |
| 4.4 Update BAML for research quality | Architecture | `baml_src/` | 4.1 | Research functions return quality signals |
| 4.5 Update chat_agent to use quality | Backend | `sandbox/src/actors/chat_agent.rs` | 4.3 | Uses ResearchQuality for completion decisions |

**Risk Register:**
- Quality signal calibration - Mitigation: Threshold tuning via config; metrics dashboard

### Phase 5: Test Modernization (Week 5)
**Goal:** Replace phrase-based test assertions with protocol assertions

| Task | Owner | File(s) | Dependencies | Acceptance Criteria |
|------|-------|---------|--------------|---------------------|
| 5.1 Update superbowl matrix test | QA | `sandbox/tests/chat_superbowl_live_matrix_test.rs` | 2.1-2.7 | Uses typed status checks; phrase matching removed |
| 5.2 Add protocol assertion helpers | QA | `sandbox/tests/` | 5.1 | Test utilities for ObjectiveStatus, CompletionPayload |
| 5.3 Define fast targeted test suite | QA | `sandbox/tests/` | 5.2 | Unit tests for each status transition |
| 5.4 Define matrix slice tests | QA | `sandbox/tests/` | 5.2 | Parameterized tests for model/provider combos |
| 5.5 Update CI test strategy | DevOps | `.github/workflows/` | 5.3-5.4 | Fast tests on PR; matrix tests on main |

**Risk Register:**
- Test coverage gaps during transition - Mitigation: Maintain both old/new assertions temporarily with warnings

### Phase 6: Integration & Cleanup (Week 6)
**Goal:** Full system integration; remove deprecated code paths

| Task | Owner | File(s) | Dependencies | Acceptance Criteria |
|------|-------|---------|--------------|---------------------|
| 6.1 Update DelegatedTaskOutcome | Architecture | `shared-types/src/lib.rs:350-360` | 1.5 | Uses CompletionPayload; backward compat layer |
| 6.2 Update all actor message handlers | Backend | `sandbox/src/actors/*.rs` | 2.1-5.5 | All handlers use typed protocols |
| 6.3 Add deprecation warnings | Backend | All refactored files | 6.2 | String-based APIs marked deprecated |
| 6.4 Update AGENTS.md policy | Architecture | `AGENTS.md` | All above | "NO ADHOC WORKFLOW" rule documented |
| 6.5 Final integration test | QA | `sandbox/tests/` | 6.4 | End-to-end workflow with typed protocols |
| 6.6 Remove deprecated code | Backend | All refactored files | 6.5 | All string-based control flow removed |

---

## 5. Risk Register & Rollback Strategy

### 5.1 Risks

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Model behavior changes with typed prompts | Medium | High | A/B testing; gradual rollout; fallback to string parsing with telemetry |
| Performance regression from type conversions | Low | Medium | Benchmark before/after; optimize hot paths |
| Test flakiness during transition | High | Medium | Maintain dual assertions temporarily; increase test timeouts |
| BAML version incompatibility | Low | High | Pin BAML version; test in isolated branch |
| Missing edge cases in enum definitions | Medium | High | Extensive property-based testing; gradual rollout |

### 5.2 Rollback Strategy

**Per-Phase Rollback:**
- Each phase maintains backward compatibility through deprecation layers
- Feature flags (`CHOIR_TYPED_PROTOCOL=1`) enable gradual rollout
- Rollback = unset feature flag + revert to deprecated APIs

**Emergency Rollback:**
```bash
# Immediate rollback to string-based control flow
git revert --no-commit HEAD~N..HEAD  # N = commits in current phase
export CHOIR_TYPED_PROTOCOL=0
just build && just test
```

**Data Compatibility:**
- Events store both typed and string representations during transition
- Database migrations are additive only
- Rollback does not lose data

---

## 6. Migration Sequence (Keep System Working)

### 6.1 Dual-Write Strategy

During transition, maintain both representations:

```rust
// Phase 2-5: Dual-write pattern
pub struct AgentPlan {
    // New typed field (primary)
    pub objective_status: ObjectiveStatus,

    // Legacy string field (deprecated, for rollback)
    #[deprecated]
    pub objective_status_legacy: Option<String>,
}

// Helper for backward compatibility
impl AgentPlan {
    pub fn objective_status_effective(&self) -> ObjectiveStatus {
        // Prefer typed; fallback to string parsing with telemetry
        self.objective_status
    }
}
```

### 6.2 Incremental Actor Migration

1. **ChatAgent** - Phase 2 (highest priority - most string matching)
2. **ResearcherActor** - Phase 4 (quality signals)
3. **TerminalActor** - Phase 3 (failure classification)
4. **Watcher** - Phase 3 (failure classification)
5. **Supervisor** - Phase 3 (timeout detection)

### 6.3 Verification Gates

| Gate | Criteria | Owner |
|------|----------|-------|
| Phase 1 Complete | All types compile; BAML sync CI passes | Architecture |
| Phase 2 Complete | ChatAgent unit tests pass; no string status checks | Backend |
| Phase 3 Complete | Watcher correctly classifies all failure types | Backend |
| Phase 4 Complete | Researcher quality signals correlate with human judgment | QA |
| Phase 5 Complete | Matrix tests pass with typed assertions | QA |
| Phase 6 Complete | Integration tests pass; deprecated code removed | All |

---

## 7. Policy Patch: AGENTS.md Additions

Add the following sections to `/Users/wiz/choiros-rs/AGENTS.md`:

```markdown
## NO ADHOC WORKFLOW Rule

**Rule:** Runtime control flow MUST NOT depend on brittle natural-language string matching of model text.

### Prohibited Patterns

```rust
// WRONG: Control flow based on phrase matching
if response.to_lowercase().contains("still running") {
    return Status::InProgress;  // Fragile!
}

// WRONG: Status determined by prefix matching
if output.starts_with("Research results for '") {
    return ResponseType::ToolDump;  // Model-dependent!
}

// WRONG: Failure classification by error text
if error.contains("timeout") || error.contains("timed out") {
    return FailureKind::Timeout;  // Inconsistent!
}
```

### Required Patterns

```rust
// CORRECT: Typed enum from structured output
match plan.objective_status {
    ObjectiveStatus::Satisfied => return Status::Complete,
    ObjectiveStatus::InProgress => return Status::Working,
    ObjectiveStatus::Blocked => return Status::Blocked,
}

// CORRECT: Failure kind from typed event
match event.failure_kind {
    FailureKind::Timeout => handle_timeout(),
    FailureKind::Network => handle_network_error(),
    _ => handle_generic_error(),
}
```

### Rationale

1. **Model Independence:** Typed protocols work across model versions and providers
2. **Determinism:** Enum values are exact; string matching is probabilistic
3. **Testability:** Typed assertions are precise; phrase assertions are brittle
4. **Maintainability:** Changing prompts shouldn't break runtime logic

### Exceptions

Input normalization (parsing user commands, config values) MAY use string matching as long as it does not control workflow state transitions.

### Enforcement

- Code review: All `contains()`, `starts_with()`, `ends_with()` on model output require explicit approval
- CI: Lint rule flags string matching on LLM response fields
- Tests: Phrase-based assertions require justification comment
```

---

## 8. External Documentation References

### 8.1 BAML Documentation

**Structured Outputs / Enums:**
- https://docs.boundaryml.com/ref/enum
- BAML enums compile to Rust enums with serde support
- Use `#[serde(rename_all = "snake_case")]` for consistent naming

**Schema Constraints:**
- https://docs.boundaryml.com/ref/class
- BAML classes generate Rust structs with `BamlDecode`/`BamlEncode` traits
- Field types are validated at deserialization time

**Runtime Behavior:**
- https://docs.boundaryml.com/ref/client
- BAML clients handle retry logic and error classification
- Custom clients can add `failure_kind` classification at the transport layer

### 8.2 Ractor Documentation

**Actor Message Contracts:**
- https://docs.rs/ractor/latest/ractor/actor/struct.Actor.html
- Messages should be typed enums, not string commands
- Use `RpcReplyPort<T>` for request-response patterns with typed replies

**State Transitions:**
- https://docs.rs/ractor/latest/ractor/actor/trait.Actor.html#tymethod.handle
- State changes should be deterministic based on message content, not string parsing
- Actor restart should resume from typed state, not re-parse strings

### 8.3 RLM/StateIndex Architecture

See internal documents:
- `/Users/wiz/choiros-rs/docs/architecture/RLM_INTEGRATION_REPORT.md` - Frame-based context management
- `/Users/wiz/choiros-rs/docs/architecture/state_index_addendum.md` - Token budgets and context compaction

Key insight: RLM's `Frame` abstraction provides natural boundaries for `ObjectiveContract` attachment. Each frame represents a unit of work with typed inputs, outputs, and status.

---

## 9. Success Metrics

| Metric | Baseline | Target | Measurement |
|--------|----------|--------|-------------|
| String-based control paths | 25+ | 0 | `grep -r "contains.*\"" sandbox/src/actors/` |
| Phrase-based test assertions | 15+ | 0 | `grep -r "contains.*\"" sandbox/tests/` |
| Typed enum variants | 0 | 20+ | Count of ObjectiveStatus, PlanMode, FailureKind variants |
| Model-agnostic test pass rate | 60% | 90% | Matrix test results across providers |
| Time to add new model | 2 days | 2 hours | No prompt engineering needed for control flow |

---

## 10. Appendix: File Reference Index

### Critical Files (Require Changes)
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs` - Primary target (lines 193-545, 1773-1777)
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs` - Quality signals (lines 313-404)
- `/Users/wiz/choiros-rs/sandbox/src/actors/watcher.rs` - Failure classification (lines 378-444)
- `/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs` - Timeout detection (line 2283)

### Type Definition Files
- `/Users/wiz/choiros-rs/shared-types/src/lib.rs` - Add ObjectiveStatus, PlanMode, FailureKind, ObjectiveContract, CompletionPayload
- `/Users/wiz/choiros-rs/baml_src/types.baml` - BAML type definitions
- `/Users/wiz/choiros-rs/baml_src/agent.baml` - BAML prompt definitions

### Test Files (Require Updates)
- `/Users/wiz/choiros-rs/sandbox/tests/chat_superbowl_live_matrix_test.rs` - Lines 312-340, 627-640

### Generated Files (Must Stay in Sync)
- `/Users/wiz/choiros-rs/sandbox/src/baml_client/baml_source_map.rs` - Embedded BAML source
- `/Users/wiz/choiros-rs/sandbox/src/baml_client/types/classes.rs` - Generated Rust types

---

*End of Document*
