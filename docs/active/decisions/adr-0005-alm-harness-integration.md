# ADR-0005: ALM Harness Integration Strategy (Draft)

Date: 2026-02-28
Kind: Decision
Status: Draft
Priority: 5
Requires: []
Owner: Core architecture + runtime

## Context

ChoirOS currently contains **two complete agent harness implementations**:

1. **AgentHarness** (`agent_harness/mod.rs`, 1,737 lines) - Linear DECIDE→EXECUTE pattern, production
2. **AlmHarness** (`agent_harness/alm.rs`, 1,298 lines) - Frame-based DAG execution, complete but unwired

The ALM (Actor Language Model) harness was designed as a more sophisticated, computationally universal execution model with:
- DAG-based multi-step execution with dependencies
- Explicit working memory across turns
- Native parallelism via `FanOut`
- Checkpoint/resume for durability
- Variable substitution and conditional gates

However, **ALM harness is fully implemented but completely unused**. All production workers (Terminal, Researcher, Writer, Conductor) use the simpler AgentHarness.

### Current State

| Component | Status | Lines | Test Coverage |
|-----------|--------|-------|---------------|
| `AlmHarness::run_turn()` | Complete, unused | 1,298 | ~150KB test files |
| `ActorAlmPort` | Complete, unused | 463 | Integration tests only |
| `execute_dag()` | Complete, unused | ~200 | DAG eval tests |
| `RlmTurn` BAML | Available, uninvoked | ~100 | None |

### The Problem

Maintaining two harness implementations creates:
- **Code drift**: ALM code is not exercised, may bit-rot
- **Confusion**: Developers unclear which harness to use
- **Maintenance burden**: Changes to shared abstractions must consider both
- **Test overhead**: 7 test files (~150KB) for unused code

## Options

### Option A: Integrate ALM into Production (Activate)

**Approach**: Wire ALM harness into Conductor and HarnessActor for complex planning.

**Specific Changes**:
1. Replace `AgentHarness` in `Conductor` with `AlmHarness` for multi-capability planning
2. Replace `HarnessActor`'s `AgentHarness` with `AlmHarness` for sub-harness delegation
3. Use `FanOut` for parallel capability dispatch (currently sequential)
4. Use DAG execution for multi-step plans with dependencies

**Pros**:
- More powerful: DAGs, parallelism, explicit memory
- Better reasoning transparency via working_memory
- Computationally universal (conditionals, loops via recursion)
- Checkpoint/resume for long-running operations

**Cons**:
- Higher complexity: context resolution phase, frame management
- More tokens per turn (model composes its own context)
- Migration risk: replacing working core orchestration
- Team learning curve

### Option B: Maintain Both (Status Quo)

**Approach**: Keep AgentHarness for simple workers, ALM for complex orchestration.

**Specific Changes**: Minimal - document when to use each.

**Pros**:
- No migration risk
- AgentHarness is proven for simple tool use
- ALM available when needed for complex cases

**Cons**:
- Continued maintenance burden
- Unclear boundaries (when is "complex enough" for ALM?)
- ALM code continues to bit-rot without production exercise
- Confusion for developers

### Option C: Deprecate and Remove ALM

**Approach**: Remove ALM harness entirely, invest in improving AgentHarness.

**Specific Changes**:
1. Delete `alm.rs`, `alm_port.rs`
2. Delete 7 ALM-specific test files (~150KB)
3. Remove `RlmTurn` BAML functions
4. Add explicit working memory to AgentHarness if needed

**Pros**:
- Single harness to maintain
- Reduced compile times, test times
- Clear architectural path
- Can always re-implement from git history if needed

**Cons**:
- Lose sophisticated capabilities (DAGs, parallelism)
- AgentHarness may need enhancement for complex cases
- Sunk cost of ALM implementation

### Option D: Merge Concepts (Hybrid)

**Approach**: Extract ALM's best ideas into AgentHarness incrementally.

**Specific Changes**:
1. Add optional DAG execution to AgentHarness (for Conductor only)
2. Add explicit working_memory field to AgentHarness turns
3. Keep linear tool execution for simple workers

**Pros**:
- Incremental improvement
- No big-bang migration
- Keeps working simple path, adds power where needed

**Cons**:
- May end up with similar complexity to ALM
- Unclear if AgentHarness architecture supports this cleanly

## Recommendation

**Option A (Activate)** for Conductor and HarnessActor, keep AgentHarness for Terminal/Researcher.

Rationale:
- ALM is already implemented and tested - the work is wiring, not coding
- Conductor is the right place for sophisticated planning (multi-step, parallel capabilities)
- AgentHarness is correct for simple tool users (Terminal, Researcher)
- The split aligns with use case complexity

## Why

1. **ALM capabilities are genuinely useful for orchestration**:
   - Parallel capability dispatch (currently sequential in Conductor)
   - Multi-step plans with dependencies (research → analyze → write)
   - Explicit working memory for long-running operations

2. **AgentHarness is correct for simple workers**:
   - Terminal just needs bash tool calls
   - Researcher just needs search/fetch tools
   - Linear flow is clearer for simple cases

3. **The implementation exists**:
   - 1,298 lines + tests already written
   - Wiring cost is lower than re-implementing from scratch later

## Consequences

### Positive
- Sophisticated orchestration capabilities
- Parallel execution reduces latency
- Explicit working memory improves debugging
- Single harness for each use case (clear mental model)

### Negative
- Two harnesses to maintain (but clear separation of concerns)
- Migration risk for Conductor (core component)
- Team needs to understand both harness patterns

## Non-Goals

- Replacing AgentHarness entirely
- Adding ALM to Terminal/Researcher (simple workers stay simple)
- Distributed ALM across multiple nodes
- Changing the BAML client architecture

## Rollout Plan (if Option A accepted)

### Phase 1: Conductor Integration (Week 1-2)
1. Add `AlmHarness` field to `ConductorState`
2. Replace sequential worker spawning with `FanOut` in Conductor
3. Add feature flag `CHOIR_CONDUCTOR_ALM_HARNESS` (default off)
4. Test with simple objectives

### Phase 2: HarnessActor Integration (Week 3)
1. Replace `AgentHarness` with `AlmHarness` in `HarnessActor`
2. Use `Recurse` for sub-harness delegation
3. Enable feature flag for sub-harness path

### Phase 3: Production Validation (Week 4)
1. Enable feature flag in dev environment
2. Run full test suite
3. Monitor for regressions
4. Gradual rollout with rollback plan

### Phase 4: Cleanup (Week 5)
1. Remove feature flag
2. Remove deprecated AgentHarness from Conductor/HarnessActor
3. Document ALM harness usage patterns

## Acceptance Criteria

1. Conductor can execute parallel capability dispatches via `FanOut`
2. Multi-step plans with dependencies work correctly
3. No regressions in existing Terminal/Researcher/Writer behavior
4. ALM harness has production usage (not just tests)
5. Team can explain when to use each harness

## Related Documents

- `simplified-agent-harness.md` - AgentHarness documentation
- `unified-agentic-loop-harness.md` - Overlapping concepts (should merge)
- `RLM_INTEGRATION_REPORT.md` - ALM architecture (now archived)
- `state_index_addendum.md` - Frame context details (now archived)
