# StateIndex Technical Addendum

**Purpose:** Deep-dive technical details for the StateIndex actor implementation.
**Companion Document:** See `RLM_INTEGRATION_REPORT.md` for architecture overview, correspondence table, and implementation plan.

---

## 1. Token Budget Operations

### Hierarchical Token Budget

Each frame maintains a `TokenBudget` with hierarchical allocation to child frames:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Total tokens allocated to this frame
    pub total: usize,
    /// Tokens used so far
    pub used: usize,
    /// Tokens reserved for specific purposes
    pub reserved: usize,
    /// Budget delegated to child frames
    pub subcall_allocation: usize,
}

impl TokenBudget {
    /// Available tokens (total - used - reserved - subcall_allocation)
    pub fn available(&self) -> usize {
        self.total
            .saturating_sub(self.used)
            .saturating_sub(self.reserved)
            .saturating_sub(self.subcall_allocation)
    }

    /// Reserve tokens for a specific purpose
    pub fn reserve(&mut self, amount: usize) -> Result<(), BudgetError> {
        if self.available() < amount {
            return Err(BudgetError::InsufficientTokens {
                requested: amount,
                available: self.available(),
            });
        }
        self.reserved += amount;
        Ok(())
    }

    /// Allocate budget for a subcall
    pub fn allocate_for_subcall(&mut self, amount: usize) -> Result<(), BudgetError> {
        if self.available() < amount {
            return Err(BudgetError::InsufficientTokens {
                requested: amount,
                available: self.available(),
            });
        }
        self.subcall_allocation += amount;
        Ok(())
    }

    /// Release subcall allocation back to parent
    pub fn release_subcall_allocation(&mut self, amount: usize) {
        self.subcall_allocation = self.subcall_allocation.saturating_sub(amount);
    }

    /// Record token usage
    pub fn record_usage(&mut self, amount: usize) {
        self.used += amount;
    }
}
```

### Budget Propagation Example

```
Root Frame (total: 8000)
├── Reserved for brief_context: 500
├── Reserved for breadcrumbs: 200
├── Subcall allocation (Child Frame): 3000
│   └── Child uses 2500, returns 500 to parent
├── Used by conversation: 2000
└── Available for evidence: 2300
```

---

## 2. Context Priority and Compaction

### Context Priority Levels

Handles and segments are tagged with priority for inclusion decisions:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextPriority {
    Critical,   // Always include (system prompts, active tool calls)
    High,       // Include unless severely constrained
    Medium,     // Include if space permits
    Low,        // Include only if abundant space
    Background, // Summarize or omit
}

impl ContextPriority {
    pub fn score(&self) -> u8 {
        match self {
            ContextPriority::Critical => 100,
            ContextPriority::High => 75,
            ContextPriority::Medium => 50,
            ContextPriority::Low => 25,
            ContextPriority::Background => 10,
        }
    }
}
```

### Compaction Levels

When context exceeds budget, apply increasing compaction:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionLevel {
    None,       // Full content, no changes
    Light,      // Minor trimming (remove whitespace, abbreviate)
    Moderate,   // Summarize older content
    Aggressive, // Heavy summarization, drop low-priority items
    Critical,   // Only critical content
}
```

### Compaction Strategies

```rust
pub enum CompactionStrategy {
    /// Remove lowest priority items first
    PriorityBased,
    /// Summarize older content
    SummarizeOldest,
    /// Truncate conversation at cutoff
    TruncateAt { message_count: usize },
    /// Custom compaction function
    Custom(Box<dyn Fn(&ContextPack) -> ContextPack + Send>),
}
```

### Compaction Flow

1. Try `None` - if fits budget, done
2. Try `Light` - trim whitespace, abbreviate long strings
3. Try `Moderate` - summarize segments older than N messages
4. Try `Aggressive` - drop Low/Background priority items
5. Try `Critical` - only Critical priority, essential breadcrumbs

---

## 3. Suspend/Resume Token Mechanism

Long-running work can be suspended and resumed later:

```rust
/// Token representing a suspended frame
pub struct SuspensionToken {
    pub token_id: String,
    pub frame_id: FrameId,
    pub suspended_at: DateTime<Utc>,
}

// Messages
SuspendFrame {
    frame_id: FrameId,
    suspension_reason: String,
    reply: RpcReplyPort<Result<SuspensionToken, StateIndexError>>,
}

ResumeFrame {
    suspension_token: SuspensionToken,
    reply: RpcReplyPort<Result<FrameId, StateIndexError>>,
}
```

### Use Cases

- **Long-running research:** Suspend while waiting for external APIs
- **User approval workflows:** Suspend while waiting for human input
- **Checkpointing:** Periodically suspend to ensure persistence

### Database Storage

```sql
CREATE TABLE IF NOT EXISTS suspended_frames (
    token_id TEXT PRIMARY KEY,
    frame_id TEXT NOT NULL UNIQUE,
    suspension_reason TEXT,
    suspended_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (frame_id) REFERENCES frames(frame_id) ON DELETE CASCADE
);
```

---

## 4. Key Design Decisions

### 4.1 Hierarchical Token Budgets

**Decision:** Each frame has its own budget with subcall allocation.
**Rationale:** Enables fine-grained control over token usage across nested work units. Parent can reserve budget for itself while delegating to children.
**Alternative Considered:** Global budget per conversation (rejected - too coarse for nested tool calls).

### 4.2 Context Segments with Priority

**Decision:** Context organized into typed segments with priorities instead of flat conversation slice.
**Rationale:** Enables intelligent compaction—keep Critical system prompts, summarize Background tool outputs.
**Alternative Considered:** Simple truncation of oldest messages (rejected - loses important context).

### 4.3 EventStore as Source of Truth

**Decision:** All frame operations logged to EventStore; StateIndex maintains projections.
**Rationale:** Enables full recovery, audit trail, and cross-actor evidence sharing. StateIndex can be rebuilt from events.
**Alternative Considered:** StateIndex as sole source of truth (rejected - lose durability if actor crashes).

### 4.4 LRU Cache for Active Stacks

**Decision:** In-memory cache of active frame stacks with bounded size.
**Rationale:** Fast access for hot stacks while keeping memory bounded. Cold stacks fetched from database on demand.
**Alternative Considered:** All stacks in memory (rejected - unbounded memory growth).

### 4.5 Suspension Tokens

**Decision:** Frames can be suspended and resumed via opaque tokens.
**Rationale:** Enables long-running work to be paused without consuming resources. Tokens can be stored, passed, or expired.
**Alternative Considered:** Simple frame status change (rejected - no way to track who/why suspended).

### 4.6 Compaction Levels

**Decision:** Explicit compaction levels with defined behavior.
**Rationale:** Predictable behavior when fitting context to budget. Caller knows what to expect at each level.
**Alternative Considered:** Automatic best-effort compaction (rejected - unpredictable, hard to debug).

---

## 5. Core Operations Pseudocode

### 5.1 Push Frame

```rust
async fn push_frame(
    &self,
    scope: Scope,
    parent_frame_id: Option<FrameId>,
    goal: String,
    inputs: Value,
    budget: TokenBudget,
) -> Result<FrameId, StateIndexError> {
    let frame_id = FrameId::new();

    // Calculate depth from parent
    let depth = if let Some(ref parent_id) = parent_frame_id {
        let parent = self.get_frame(parent_id).await?
            .ok_or_else(|| StateIndexError::ParentFrameNotFound(...))?;

        // Enforce depth limit
        if parent.depth >= parent.budget.max_subframe_depth {
            return Err(StateIndexError::MaxDepthExceeded);
        }

        parent.depth + 1
    } else {
        0
    };

    let frame = Frame {
        frame_id: frame_id.clone(),
        parent_frame_id,
        scope: scope.clone(),
        goal,
        inputs,
        status: FrameStatus::Active,
        depth,
        budget,
        context_handles: Vec::new(),
        result_refs: Vec::new(),
        created_at: Utc::now(),
        completed_at: None,
        context_hash: None,
    };

    // Persist to database
    self.persist_frame(&frame).await?;

    // Log event to EventStore
    self.log_event("frame.pushed", &frame).await?;

    // Update cache
    self.update_cached_stack(&scope, frame);

    Ok(frame_id)
}
```

### 5.2 Assemble Context Pack

```rust
async fn assemble_context_pack(
    &self,
    request: AssembleContextPackRequest,
) -> Result<ContextPack, StateIndexError> {
    let scope = request.scope;

    // Determine target frame
    let target_frame_id = match request.frame_id {
        Some(id) => id,
        None => self.find_top_of_stack(&scope).await?
            .ok_or_else(|| StateIndexError::FrameNotFound("No active frame".to_string()))?,
    };

    // Get frame and ancestry
    let ancestry = self.get_frame_ancestry(&target_frame_id).await?;
    let target_frame = ancestry.last().ok_or_else(|| ...)?;

    // Check budget minimum
    if request.budget_tokens < 500 {
        return Err(StateIndexError::BudgetTooSmall);
    }

    let mut remaining_budget = request.budget_tokens;

    // 1. Assemble brief context (~500 tokens reserved)
    let brief_context = self.assemble_brief_context(&request.role, &scope).await;
    let brief_tokens = estimate_tokens(&brief_context);
    remaining_budget = remaining_budget.saturating_sub(brief_tokens + 50);

    // 2. Build breadcrumbs (minimal token cost)
    let breadcrumbs: Vec<FrameBreadcrumb> = ancestry.iter()
        .map(|f| FrameBreadcrumb { ... })
        .collect();
    let breadcrumb_tokens = breadcrumbs.len() * 40;
    remaining_budget = remaining_budget.saturating_sub(breadcrumb_tokens);

    // 3. Assemble segments from context handles by priority
    let mut segments = Vec::new();
    let mut segment_tokens = 0;

    // Sort handles by priority (Critical first)
    let mut handles: Vec<_> = target_frame.context_handles.iter()
        .filter(|h| !request.query_hints.exclude_handles.contains(&h.handle_id))
        .collect();
    handles.sort_by_key(|h| std::cmp::Reverse(h.priority.score()));

    // Include mandatory handles first
    for handle in &handles {
        if request.query_hints.include_handles.contains(&handle.handle_id)
            || handle.priority == ContextPriority::Critical {
            let content = self.retrieve_handle(handle).await?;
            let tokens = estimate_tokens(&content);

            if segment_tokens + tokens > remaining_budget * 3 / 4 {
                // Would exceed budget, skip non-critical
                if handle.priority != ContextPriority::Critical {
                    continue;
                }
            }

            segments.push(ContextSegment { ... });
            segment_tokens += tokens;
        }
    }

    // Apply compaction if over budget
    let (final_segments, compaction_level) = if brief_tokens + breadcrumb_tokens + segment_tokens > request.budget_tokens {
        self.compact_segments(segments, remaining_budget, request.min_compaction).await?
    } else {
        (segments, CompactionLevel::None)
    };

    let total_used = brief_tokens + breadcrumb_tokens
        + final_segments.iter().map(|s| s.estimated_tokens).sum::<usize>()
        + 50;

    Ok(ContextPack {
        metadata: ContextPackMetadata { ... },
        brief_context,
        breadcrumbs,
        segments: final_segments,
        token_summary: TokenSummary {
            budget: request.budget_tokens,
            used: total_used,
            remaining: request.budget_tokens.saturating_sub(total_used),
            breakdown: TokenBreakdown { ... },
            compaction_level,
        },
    })
}
```

### 5.3 Resume Actor

```rust
async fn resume_actor(
    &self,
    scope: Scope,
) -> Result<ActorResumeState, StateIndexError> {
    let start_time = std::time::Instant::now();

    // Find top of stack
    let top_frame_id = self.find_top_of_stack(&scope).await?;

    let (frame_stack, pending_work) = if let Some(frame_id) = top_frame_id {
        // Rebuild frame stack from leaf to root
        let ancestry = self.get_frame_ancestry(&frame_id).await?;

        // Detect pending work
        let pending = self.detect_pending_work(&ancestry).await;

        (ancestry, pending)
    } else {
        (Vec::new(), Vec::new())
    };

    let current_frame = frame_stack.last().cloned();

    Ok(ActorResumeState {
        current_frame,
        frame_stack,
        pending_work,
        recovery_summary: RecoverySummary {
            frames_recovered: frame_stack.len(),
            pending_work_items: pending_work.len(),
            last_event_seq: self.last_seq,
            recovery_time_ms: start_time.elapsed().as_millis() as u64,
        },
    })
}

async fn detect_pending_work(
    &self,
    frame_stack: &[Frame],
) -> Vec<PendingWork> {
    let mut pending = Vec::new();

    for frame in frame_stack {
        match frame.status {
            FrameStatus::Waiting => {
                pending.push(PendingWork {
                    frame_id: frame.frame_id.clone(),
                    work_type: PendingWorkType::WaitingForSubcall,
                    description: format!("Frame '{}' waiting", frame.goal),
                });
            }
            FrameStatus::Active => {
                // Check for incomplete tool calls
                let incomplete = self.find_incomplete_tools(frame).await?;
                for tool in incomplete {
                    pending.push(PendingWork {
                        frame_id: frame.frame_id.clone(),
                        work_type: PendingWorkType::ToolInProgress,
                        description: format!("Tool '{}' incomplete", tool),
                    });
                }
            }
            _ => {}
        }
    }

    Ok(pending)
}
```

### 5.4 Compact Context

```rust
async fn compact_context(
    &self,
    frame_id: FrameId,
    target_budget: usize,
    strategy: CompactionStrategy,
) -> Result<CompactionResult, StateIndexError> {
    let frame = self.get_frame(&frame_id).await?
        .ok_or_else(|| StateIndexError::FrameNotFound(...))?;

    let original_tokens = frame.budget.used;

    match strategy {
        CompactionStrategy::PriorityBased => {
            // Sort handles by priority, drop lowest first
            let mut handles = frame.context_handles.clone();
            handles.sort_by_key(|h| h.priority.score());

            let mut current_tokens = original_tokens;
            let mut items_removed = 0;

            for handle in handles {
                if current_tokens <= target_budget {
                    break;
                }
                if handle.priority == ContextPriority::Critical {
                    continue; // Never drop critical
                }

                let tokens = handle.estimated_tokens.unwrap_or(100);
                current_tokens -= tokens;
                items_removed += 1;

                // Actually remove from frame
                self.remove_handle(&frame_id, &handle.handle_id).await?;
            }

            Ok(CompactionResult {
                original_tokens,
                final_tokens: current_tokens,
                compaction_level: self.determine_compaction_level(original_tokens, current_tokens),
                items_removed,
                items_summarized: 0,
            })
        }
        CompactionStrategy::SummarizeOldest => {
            // Summarize older messages, keep recent N
            let recent_n = 10;
            let mut items_summarized = 0;

            // Implementation: convert older messages to summaries
            // Keep last N messages intact

            Ok(CompactionResult { ... })
        }
        // ... other strategies
    }
}
```

---

## 6. Event Types Reference

Complete list of StateIndex events:

| Event | Payload | Description |
|-------|---------|-------------|
| `frame.pushed` | `FramePushedEvent` | New frame created |
| `frame.updated` | `FrameUpdatedEvent` | Frame status/fields changed |
| `frame.popped` | `FramePoppedEvent` | Frame completed/failed |
| `frame.suspended` | `FrameSuspendedEvent` | Frame suspended with token |
| `frame.resumed` | `FrameResumedEvent` | Suspended frame resumed |
| `frame.handle_added` | `HandleAddedEvent` | Context handle added |
| `frame.result_added` | `ResultAddedEvent` | Result reference added |
| `frame.compacted` | `FrameCompactedEvent` | Context compacted to fit budget |
| `actor.resumed` | `ActorResumedEvent` | Actor resumed from restart |

---

## 7. Integration Checklist

When integrating StateIndex with an actor:

- [ ] Add `state_index: Option<ActorRef<StateIndexMsg>>` to actor arguments
- [ ] In `pre_start`, call `ResumeActor` to check for previous state
- [ ] On new work, call `PushFrame` to create a frame
- [ ] Before LLM calls, call `AssembleContextPack` for bounded context
- [ ] After tool calls, call `AddContextHandle` to store results
- [ ] On completion, call `PopFrame` with final status
- [ ] Handle `PendingWork` items on resume appropriately
