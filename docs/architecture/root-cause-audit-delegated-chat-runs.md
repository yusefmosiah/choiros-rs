# Root-Cause Audit: Delegated Chat Run Failures

**Date:** 2026-02-08 (Updated 2026-02-09)
**Audited System:** /Users/wiz/choiros-rs

## Narrative Summary (1-minute read)

This audit identifies systemic failures in chat delegation paths where user intent is progressively truncated and completion is signaled by proxy metrics rather than true fulfillment. The findings describe violations of the NO ADHOC WORKFLOW policy: string-matching heuristics and token-coverage thresholds are used for workflow state transitions instead of typed protocol fields. Chat should remain a thin compatibility surface; multi-step orchestration belongs in Conductor with proper typed contracts.

## What Changed

- Documented proxy-metric collapse (token coverage != intent fulfillment).
- Identified progressive intent truncation at every delegation layer.
- Catalogued race conditions in async followup lifecycle management.
- Added explicit NO ADHOC WORKFLOW policy context per AGENTS.md architecture direction.

## What To Do Next

1. **Immediate**: Preserve full objective context in Researcher delegation (researcher.rs:266).
2. **Short-term**: Remove double truncation in objective extraction (chat_agent.rs:221-227).
3. **Medium-term**: Replace token-coverage completion heuristic with semantic validation.
4. **Architectural**: Migrate multi-step planning from ChatAgent to Conductor with typed contracts.

---

**Focus Areas:**
- Objective propagation fidelity (chat → supervisor → researcher/terminal → followup synthesis)
- Final-answer gating logic ("done" vs "tool dump")
- Event/timeline races (async followup, worker completion, quiet/timeout exits)
- Citation coverage vs intent fulfillment confusion

---

## Executive Summary

The ChoirOS agentic system suffers from a **systemic pattern of optimizing for measurable proxy metrics over true intent fulfillment**. At every delegation layer, user intent is truncated, generalized, and eventually replaced by structural signals of completion (citation counts, token coverage percentages, non-empty response fields). The result is "completion theater"—runs that appear successful in logs and metrics but fail to actually answer the user's original question.

---

## Top 5 Root-Cause Candidates (Ranked by Confidence)

---

### 1. Proxy Chain Collapse: Token Coverage Heuristic Conflated with Intent Fulfillment

**Confidence:** HIGH

**Exact file:function:line references:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs:assess_objective_completion:313-404`
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs:relevance_tokens:294-311`
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs:400`

**Failure mechanism:**

The system uses superficial token-matching (35% coverage threshold) to determine research completion instead of semantic intent validation. Citations are scored by presence of query tokens in title/snippet/URL via simple substring search with a hardcoded stopword list:

```rust
const STOP: &[&str] = &["the", "a", "an", "and", "or", "for", "to", "of", ...];

// Lines 347-358: Simple substring search
if citations.iter().any(|c| {
    let haystack = format!(
        "{} {} {}",
        c.title.to_ascii_lowercase(),
        c.snippet.to_ascii_lowercase(),
        c.url.to_ascii_lowercase()
    );
    haystack.contains(token)
}) {
    matched += 1;
}
```

A query like "Is bitcoin a good investment?" matches tokens "bitcoin" in citations about "Bitcoin mining environmental impact" without ever addressing investment potential. The system considers the objective "Complete" when 35% of query tokens appear in citations:

```rust
// Line 400: COMPLETE status granted based on coverage threshold
(
    ResearchObjectiveStatus::Complete,
    "Sufficient citation coverage for objective completion.".to_string(),
    None,
    None,
)
```

Fixed confidence scores (0.72, 0.62) at lines 904 and 917 further mask the lack of true intent assessment:

```rust
// Lines 898-908: Findings generated with FIXED confidence
.map(|citation| shared_types::WorkerFinding {
    confidence: 0.72,  // ARBITRARY FIXED VALUE
    ...
})
```

**Minimal fix:**

Replace token-coverage heuristic with an LLM-based intent-fulfillment check in `assess_objective_completion`. Add a secondary validation step that passes top citations to a lightweight model with the original objective, asking: "Does this evidence directly answer the user's objective? Reply yes/no with explanation."

---

### 2. Progressive Intent Truncation Through Layered Defensive Coding

**Confidence:** HIGH

**Exact file:function:line references:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:objective_contract:214-218` (600 char truncation)
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:objective_primary_from_contract:221-227` (400 char re-truncation)
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs:ResearcherMsg::RunAgenticTask:265-267` (objective discarded, becomes query-only)

**Failure mechanism:**

User intent undergoes compounding truncation at every delegation layer:

```
User message (potentially 1000+ chars)
    -> 600 chars (objective_contract at line 217)
    -> 400 chars (objective_primary_from_contract at line 225)
    -> Delegated to researcher
    -> NONE (researcher.rs:266 sets objective: None)
```

The truncation at line 217:

```rust
fn objective_contract(user_prompt: &str) -> String {
    format!(
        "Objective Contract\nPrimary objective: {}\nCompletion rule: ...",
        Self::truncate_for_chat(user_prompt, 600)  // CRITICAL: 600 char limit
    )
}
```

Double truncation at line 225:

```rust
fn objective_primary_from_contract(contract: &str) -> String {
    contract
        .lines()
        .find_map(|line| line.strip_prefix("Primary objective: "))
        .map(|line| Self::truncate_for_chat(line, 400))  // Truncated AGAIN
        .unwrap_or_else(|| Self::truncate_for_chat(contract, 400))
}
```

And finally at the researcher, the objective is stripped entirely:

```rust
// researcher.rs:265-276
let request = ResearcherWebSearchRequest {
    query: objective,  // The objective text becomes just a "query"
    objective: None,   // The objective field is EMPTY!
    ...
};
```

By the time research executes, the original nuanced intent is lost—constraints, context, and specific requirements beyond ~400 characters are silently discarded.

**Minimal fix:**

Remove truncation from `objective_primary_from_contract` (line 225) - the contract is already truncated at 600 chars. At researcher.rs:266, change `objective: None` to `objective: Some(objective)` to preserve the full intent context for the researcher's completion assessment.

---

### 3. Structural Completion Conflated with Semantic Fulfillment in Tool Fallbacks

**Confidence:** HIGH

**Exact file:function:line references:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:fallback_response_from_tool_results:435-462`
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:objective_is_satisfied_for_plan:357-368`
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:1907-1912` (fallback invocation)

**Failure mechanism:**

When the LLM fails to produce a `final_response` in the PlanAction loop, the system falls back to returning raw tool output directly without synthesis:

```rust
// Lines 435-462
fn fallback_response_from_tool_results(
    user_prompt: &str,
    tool_results: &[ToolResult],
    last_thinking: &str,
) -> String {
    if let Some(success) = tool_results
        .iter()
        .rev()
        .find(|result| result.success && !result.output.trim().is_empty())
    {
        return Self::truncate_for_chat(&success.output, 1200);  // RAW TOOL OUTPUT RETURNED
    }
    // ... error cases
}
```

This is invoked at lines 1907-1908 when `final_response_from_plan` is `None`:

```rust
let text = final_response_from_plan.unwrap_or_else(|| {
    Self::fallback_response_from_tool_results(user_prompt, &tool_results, &last_thinking)
});
```

The compatibility bridge at lines 357-368 allows completion when `has_final_response && !has_tool_calls`, even if the response doesn't actually satisfy the objective:

```rust
fn objective_is_satisfied_for_plan(
    status: Option<&str>,
    has_final_response: bool,
    has_tool_calls: bool,
) -> bool {
    if Self::objective_status_is_satisfied(status) {
        return true;
    }
    // Compatibility bridge for models that have not fully adopted objective_status
    status.is_none() && has_final_response && !has_tool_calls  // FALLBACK GAP
}
```

A model can emit any text as `final_response` and the system considers the objective satisfied. The user receives a tool dump (research citations, terminal command output) instead of an answer.

**Minimal fix:**

In `fallback_response_from_tool_results`, before returning raw tool output, check if the output actually addresses the user's prompt. If not, return a status indicating incomplete rather than a faux-complete answer. Remove or tighten the compatibility bridge at line 367 to require explicit `objective_status: satisfied` for completion.

---

### 4. Orphaned Background Tasks Without Lifecycle Management

**Confidence:** MEDIUM-HIGH

**Exact file:function:line references:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:spawn_background_followup:1926-2054`
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:wait_for_delegated_task_result_internal:3091-3190`
- `/Users/wiz/choiros-rs/sandbox/src/supervisor/mod.rs:1083-1144` (terminal task timeout)

**Failure mechanism:**

Background followup tasks spawned via `tokio::spawn` at line 1941 have no cancellation mechanism if the chat agent restarts:

```rust
fn spawn_background_followup(...) {
    tokio::spawn(async move {  // Detached task, no lifecycle management
        let result = Self::wait_for_delegated_task_result_internal(...).await;
        // ... process result and emit event
        let _ = ractor::call!(event_store, |reply| EventStoreMsg::Append { event, reply });
    });
}
```

If the chat agent restarts (due to supervision), the background task continues running but has no parent to report to, leading to "ghost" followup messages appearing after the user has already received a response.

Additionally, the supervisor sets a hard deadline that may fire during active processing:

```rust
// supervisor/mod.rs:1083-1144
let hard_deadline = start_time + std::time::Duration::from_millis(timeout_ms.saturating_add(20_000));
loop {
    tokio::select! {
        Some(progress) = progress_rx.recv() => { /* handle progress */ }
        _ = tokio::time::sleep_until(hard_deadline) => {
            // Timeout fires even if progress just happened
            Self::publish_worker_event(..., EVENT_TOPIC_WORKER_TASK_FAILED, ...);
            return;
        }
    }
}
```

If a worker emits progress just before the hard deadline, the timeout may fire during active processing.

**Minimal fix:**

Add a cancellation token to `spawn_background_followup` that is stored in actor state and cancelled on actor shutdown. Add a heartbeat mechanism in the polling loop that fails fast if no "running" events are seen within a sub-timeout (e.g., 30s), rather than waiting for the full hard deadline. Use sliding window timeouts that extend based on worker progress rather than fixed hard deadlines.

---

### 5. Delegation Context Stripping in Research Handoff

**Confidence:** MEDIUM

**Exact file:function:line references:**
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:1455-1484` (delegation site)
- `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:1461` (objective extraction)
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs:266` (objective becomes None)

**Failure mechanism:**

When delegating to the researcher, the completion rules and fallback rules from the objective contract are stripped. Only the primary objective text is passed:

```rust
// chat_agent.rs:1455-1484
async fn delegate_research_tool_sync_followup(
    ...
    objective_contract: &str,
) -> Result<ToolOutput, ChatAgentError> {
    ...
    let objective = Some(Self::objective_primary_from_contract(objective_contract));  // Just primary text
    ...
    let task = ractor::call!(supervisor, |reply| {
        ApplicationSupervisorMsg::DelegateResearchTask {
            ...
            objective,  // Only the 400-char primary objective, no completion rules
```

The full objective contract (with completion rules and fallback guidance) is reduced to just the "Primary objective" text before delegation. The researcher never sees the completion rules or fallback guidance, leading to premature completion declarations.

**Minimal fix:**

Pass the full objective contract (not just primary objective) to the researcher. Add a field `completion_criteria: String` to `ResearcherWebSearchRequest` that includes the completion and fallback rules from the original contract.

---

## Most Likely Single Key Error

> **The system optimizes for measurable proxy metrics (token coverage, citation count, response field presence) instead of unmeasurable intent fulfillment, creating a "completion theater" where structural signals of completion are mistaken for actual user satisfaction.**

---

## Concrete Patch Plan

Prioritized by impact-to-effort ratio:

---

### Patch 1: Preserve Full Objective in Research Delegation (Highest Impact/Effort Ratio)

**File:** `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs:265-276`

```rust
// BEFORE (lines 265-276):
let request = ResearcherWebSearchRequest {
    query: objective,
    objective: None,  // CRITICAL: Intent lost here
    ...
};

// AFTER:
let request = ResearcherWebSearchRequest {
    query: objective.clone(),
    objective: Some(objective),  // Preserve the full objective for assessment
    ...
};
```

**Impact:** High - restores intent context for completion assessment
**Effort:** Minimal - single line change

---

### Patch 2: Remove Double Truncation in Objective Extraction

**File:** `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:221-227`

```rust
// BEFORE:
fn objective_primary_from_contract(contract: &str) -> String {
    contract
        .lines()
        .find_map(|line| line.strip_prefix("Primary objective: "))
        .map(|line| Self::truncate_for_chat(line, 400))  // Unnecessary re-truncation
        .unwrap_or_else(|| Self::truncate_for_chat(contract, 400))
}

// AFTER:
fn objective_primary_from_contract(contract: &str) -> String {
    contract
        .lines()
        .find_map(|line| line.strip_prefix("Primary objective: "))
        .map(|line| line.to_string())  // Already truncated to 600 in contract
        .unwrap_or_else(|| Self::truncate_for_chat(contract, 600))
}
```

**Impact:** Medium-High - preserves more intent context
**Effort:** Minimal - remove truncation call

---

### Patch 3: Add Intent-Awareness to Tool Fallback

**File:** `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:435-462`

```rust
// BEFORE (lines 440-446):
if let Some(success) = tool_results
    .iter()
    .rev()
    .find(|result| result.success && !result.output.trim().is_empty())
{
    return Self::truncate_for_chat(&success.output, 1200);  // Raw dump
}

// AFTER:
if let Some(success) = tool_results
    .iter()
    .rev()
    .find(|result| result.success && !result.output.trim().is_empty())
{
    // Check if output actually addresses the prompt before returning
    let output = &success.output;
    if Self::output_addresses_prompt(output, user_prompt) {
        return Self::truncate_for_chat(output, 1200);
    }
    // Fall through to incomplete status rather than fake completion
}
```

**Impact:** High - prevents false completion signals
**Effort:** Low-Medium - requires adding `output_addresses_prompt` helper

---

### Patch 4: Replace Token Coverage with Semantic Completion Check

**File:** `/Users/wiz/choiros-rs/sandbox/src/actors/researcher.rs:379-404`

```rust
// BEFORE (lines 379-404):
let low_confidence = avg_score.map(|s| s < 0.35).unwrap_or(false);
let weak_coverage = coverage < 0.35;

if provider_successes == 0 || weak_coverage || low_confidence {
    return (ResearchObjectiveStatus::Incomplete, ...);
}

(
    ResearchObjectiveStatus::Complete,  // Based only on coverage
    "Sufficient citation coverage for objective completion.".to_string(),
    ...
)

// AFTER:
let low_confidence = avg_score.map(|s| s < 0.35).unwrap_or(false);
let weak_coverage = coverage < 0.35;

if provider_successes == 0 || weak_coverage || low_confidence {
    return (ResearchObjectiveStatus::Incomplete, ...);
}

// Add semantic validation: do citations actually answer the objective?
let intent_fulfilled = Self::citations_answer_objective(citations, query);
if !intent_fulfilled {
    return (
        ResearchObjectiveStatus::Incomplete,
        "Citations found but do not directly answer the objective.".to_string(),
        Some("terminal".to_string()),
        Some(format!("Verify with terminal tools: {}", query)),
    );
}

(
    ResearchObjectiveStatus::Complete,
    "Citations semantically satisfy the objective.".to_string(),
    ...
)
```

**Impact:** Very High - addresses root cause of false completions
**Effort:** Medium - requires implementing `citations_answer_objective` (can use lightweight LLM call or heuristic)

---

### Patch 5: Add Cancellation Tokens to Background Followups

**File:** `/Users/wiz/choiros-rs/sandbox/src/actors/chat_agent.rs:1926-2050`

```rust
// BEFORE (line 1941):
tokio::spawn(async move {
    let result = Self::wait_for_delegated_task_result_internal(...).await;
    ...
});

// AFTER:
let cancel_token = state.cancel_tokens.entry(task_id.clone()).or_insert_with(|| CancellationToken::new());
let token_clone = cancel_token.clone();
tokio::spawn(async move {
    tokio::select! {
        result = Self::wait_for_delegated_task_result_internal(...) => { ... }
        _ = token_clone.cancelled() => {
            // Clean shutdown, log cancellation
            return;
        }
    }
});
// On actor shutdown, cancel all tokens
```

**Impact:** Medium - prevents orphaned tasks and resource leaks
**Effort:** Medium - requires adding cancellation token management to actor state

---

## Appendix: Detailed Finding Summaries by Audit Area

### Intent Propagation Audit Summary

| Finding | Location | Severity | Impact |
|---------|----------|----------|--------|
| Objective contract truncation | chat_agent.rs:217 | CRITICAL | 600 char hard limit on user intent |
| Double truncation in delegation | chat_agent.rs:225 | CRITICAL | 400 char limit on delegated objectives |
| Context preview truncation | chat_agent.rs:1250 | CRITICAL | 320 char limit on planning context |
| Objective becomes query | researcher.rs:266 | HIGH | Semantic meaning lost |
| Research delegation strips rules | chat_agent.rs:1461 | HIGH | Completion rules not passed |
| Terminal reasoning truncation | chat_agent.rs:1338 | MEDIUM | 260 char limit on rationale |
| Tool observation truncation | chat_agent.rs:420 | MEDIUM | 800 char limit on tool output |
| Outcome truncation | chat_agent.rs:2380 | MEDIUM | 800 char limit on outcomes |
| BAML no raw user message | agent.baml:5 | MEDIUM | Planner sees truncated context |
| Followup output truncation | chat_agent.rs:1622 | MEDIUM | 1200 char limit on results |

**Root cause:** Defensive truncation at every layer compounds information loss.

### Final Answer Gating Audit Summary

| Issue | Severity | File:Line | Failure Mode |
|-------|----------|-----------|--------------|
| Tool dump fallback | CRITICAL | chat_agent.rs:435-462 | Returns raw tool output when synthesis fails |
| Token coverage heuristic | HIGH | researcher.rs:313-404 | 35% token match != semantic answer |
| Compatibility bridge | HIGH | chat_agent.rs:357-368 | No objective_status = auto-complete |
| Intent alignment gaps | MEDIUM | chat_agent.rs:272-305 | Pattern matching, not semantic validation |
| Citation list as summary | MEDIUM | researcher.rs:863-889 | Bibliography returned as answer |
| Score threshold issues | LOW | researcher.rs:366-377 | Binary threshold on noisy scores |

**Root cause:** Structural completion conflated with semantic fulfillment.

### Race Condition Audit Summary

| Rank | Issue | Location | Severity |
|------|-------|----------|----------|
| 1 | Background Followup Orphaning | chat_agent.rs:1926 | CRITICAL |
| 2 | Worker Completion vs Timeout Race | supervisor/mod.rs:1083 | CRITICAL |
| 3 | Quiet Exit Mistakes Inactivity | chat_superbowl_live_matrix_test.rs:496 | HIGH |
| 4 | Event Ordering in Followup | event_relay.rs:153 | HIGH |
| 5 | Soft/Hard Wait Race | chat_agent.rs:2854 | MEDIUM |
| 6 | Worker Report State Sync | supervisor/mod.rs:2405 | MEDIUM |
| 7 | Terminal Exit Double Send | terminal.rs:1146 | LOW-MED |
| 8 | Relay Cursor Advancement | event_relay.rs:153 | LOW |

**Root cause:** Task lifecycle management lacks proper cancellation/heartbeats.

### Citation vs Intent Audit Summary

| Finding | Severity | File:Line | Mechanism |
|---------|----------|-----------|-----------|
| Completion by coverage % | CRITICAL | researcher.rs:400 | Token match >= 35% = COMPLETE |
| Superficial token matching | CRITICAL | researcher.rs:294-311 | Stopword filtering + substring search |
| Fixed confidence scores | HIGH | researcher.rs:904,917 | 0.72/0.62 regardless of quality |
| Result count metrics | HIGH | researcher.rs:68-95 | Progress tracks counts, not relevance |
| Default max_results=6 | MEDIUM | researcher.rs:538 | Assumes more is better |
| Score thresholding | MEDIUM | researcher.rs:366-380 | Provider scores != intent fulfillment |
| List-based summaries | LOW | researcher.rs:876-887 | No synthesis validation |

**Root cause:** Proxy chain collapse - optimizing for measurable citations over unmeasurable intent fulfillment.

---

## Conclusion

The audit findings reveal a systemic pattern across all layers of the agentic system:

1. **At input:** User intent is truncated to fit arbitrary character limits
2. **At delegation:** Context is stripped to minimal query strings
3. **At execution:** Success is measured by proxy metrics (coverage, counts)
4. **At completion:** Structural signals are mistaken for semantic fulfillment
5. **At cleanup:** Race conditions allow orphaned tasks and premature exits

The recommended patches progress from quick fixes (preserving intent context) to structural improvements (semantic validation), with Patch 1 offering the highest immediate impact for minimal effort.

**Next steps:**
1. Apply Patch 1 (preserve objective) immediately
2. Apply Patch 2 (remove double truncation) immediately
3. Implement `citations_answer_objective` for Patch 4 (semantic validation)
4. Add telemetry to measure actual vs perceived intent fulfillment
5. Consider LLM-based intent extraction at entry point to capture nuanced requirements
