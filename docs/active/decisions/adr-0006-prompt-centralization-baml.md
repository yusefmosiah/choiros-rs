# ADR-0006: Prompt Centralization in BAML (Draft)

Date: 2026-02-28
Status: Draft (decision pending)
Owner: Core architecture + developer experience

## Context

ChoirOS currently has a **hybrid prompt architecture** with significant technical debt:

- **BAML files** (`baml_src/`): Centralize some prompts with type safety
- **Rust source code**: Contains ~500 lines of hardcoded prompts across 7+ files

### Scattered Prompts Inventory

| File | Prompt Type | Lines | Risk |
|------|-------------|-------|------|
| `conductor_adapter.rs:38` | Conductor routing guidance | ~30 | CRITICAL |
| `model_gateway.rs:20` | Duplicate conductor guidance | ~10 | CRITICAL |
| `researcher/adapter.rs:908` | Research agent system prompt | ~60 | CRITICAL |
| `terminal.rs:654` | Terminal agent context | ~25 | HIGH |
| `writer/adapter.rs:230` | Writer delegation prompt | ~15 | HIGH |
| `harness_actor/adapter.rs:251` | Sub-harness prompt | ~20 | HIGH |
| `agent_harness/mod.rs:1616` | Default harness fallback | ~1 | LOW |

**Plus**: ~190 lines of tool descriptions hardcoded in Rust.

### The Problem

**Maintenance hazards**:
- **Version drift**: Prompt changes require recompilation and redeployment
- **No A/B testing**: Cannot easily test prompt variations
- **Duplication**: Conductor routing guidance exists in 2 places
- **Hidden complexity**: Tool schemas embedded with `format!()` interpolation
- **Inconsistent formatting**: Inline strings vs BAML templates

**Developer friction**:
- Prompt engineering requires Rust changes
- No separation of concerns (logic vs language)
- Difficult to review prompt changes in PRs

## Decision

**Centralize all production prompts in BAML.**

Create `baml_src/agent_prompts.baml` as the single source of truth for agent system prompts.

## Why

1. **Maintainability**: Prompt changes without recompilation
2. **Version control**: BAML files track prompt versions independently
3. **Testing**: Can A/B test prompts via BAML client switching
4. **Consistency**: All prompts use same templating language
5. **Safety**: BAML type checking prevents malformed prompts
6. **Developer workflow**: Prompt engineers don't need Rust knowledge

## Consequences

### Positive
- Single source of truth for all LLM interactions
- Faster prompt iteration (no recompile)
- Type-safe prompt templates
- Clear separation: Rust for logic, BAML for language
- Observable: BAML collector tracks prompt usage

### Negative
- Migration effort: ~170 lines of Rust to convert
- Behavioral risk: Template rendering differences between `format!()` and BAML
- Team learning: Developers must understand BAML templating
- Performance: Additional BAML client call overhead (minimal)

## Non-Goals

- Moving test prompts to BAML (test-only prompts can stay inline)
- Changing the LLM client architecture
- Adding new prompt versioning infrastructure (use git)
- Replacing BAML with another system

## Rollout Plan

### Phase 1: Conductor Prompts (Week 1)
**Priority**: CRITICAL (duplicated guidance)

1. Create `baml_src/agent_prompts.baml`
2. Create `ConductorHarnessRoute` function
3. Merge the two conductor routing prompts into one BAML function
4. Replace `CONDUCTOR_ROUTING_GUIDANCE` const with BAML call
5. Remove duplicate from `model_gateway.rs`

**Files changed**:
- `conductor_adapter.rs` (-30 lines, +5 lines)
- `model_gateway.rs` (-10 lines)
- New: `baml_src/agent_prompts.baml` (~40 lines)

### Phase 2: Researcher Prompt (Week 1-2)
**Priority**: CRITICAL (largest prompt)

1. Create `ResearchAgentSystemPrompt` function
2. Handle dynamic sections (`run_doc_hint`) via BAML template parameters
3. Replace 60-line `format!()` string with BAML call

**Challenge**: The researcher prompt has conditional sections based on mode.

**Solution**: Use BAML conditionals:
```baml
{% if run_doc_hint %}
Run document: {{ run_doc_hint }}
{% endif %}
```

**Files changed**:
- `researcher/adapter.rs` (-60 lines, +10 lines)

### Phase 3: Worker Prompts (Week 2)
**Priority**: HIGH

1. `TerminalAgentPrompt` - handle `writer_mode` conditional
2. `WriterDelegationPrompt` - simple replacement
3. `SubHarnessPrompt` - simple replacement

**Files changed**:
- `terminal.rs` (-25 lines, +8 lines)
- `writer/adapter.rs` (-15 lines, +5 lines)
- `harness_actor/adapter.rs` (-20 lines, +5 lines)

### Phase 4: Tool Descriptions (Week 3)
**Priority**: MEDIUM

1. Create BAML types for tool schemas
2. Generate tool JSON schemas from BAML
3. Eliminate hardcoded tool descriptions

**Files changed**:
- 6 files with tool descriptions (~190 lines)
- New: `baml_src/tool_schemas.baml`

### Phase 5: Testing & Validation (Week 4)

1. **Regression testing**: Ensure BAML prompts produce identical outputs
2. **A/B testing**: Run conductor routing decisions side-by-side
3. **Validation**: Researcher behavior unchanged across test suite
4. **Rollback**: Feature flags for gradual rollout

## Implementation Details

### New BAML File Structure

```
baml_src/
├── agent_prompts.baml          # System prompts
│   ├── ConductorHarnessRoute
│   ├── ResearchAgentSystemPrompt
│   ├── TerminalAgentPrompt
│   ├── WriterDelegationPrompt
│   └── SubHarnessPrompt
└── tool_schemas.baml           # Tool descriptions
    ├── ResearchTools
    ├── TerminalTools
    └── WriterTools
```

### BAML Template Pattern

For prompts with dynamic sections:

```baml
function ResearchAgentSystemPrompt(
  objective: string,
  run_doc_hint: string?,
  tool_list: string,
) -> string {
  client Default
  prompt #"
    You are a research agent. Your goal is to gather information...

    Objective: {{ objective }}

    {% if run_doc_hint %}
    Working document: {{ run_doc_hint }}
    {% endif %}

    Available tools:
    {{ tool_list }}
  "#
}
```

### Rust Calling Pattern

Replace:
```rust
const PROMPT: &str = r#"..."#;
format!("{}{}", PROMPT, context)
```

With:
```rust
let prompt = baml_client
    .ResearchAgentSystemPrompt(objective, run_doc_hint, tool_list)
    .await?;
```

## Acceptance Criteria

1. **Zero inline system prompts** in production Rust code
2. **Single source of truth**: All prompts in BAML files
3. **No behavioral regressions**: Test suite passes unchanged
4. **Team documentation**: README updated with BAML prompt workflow
5. **Developer can modify prompts** without touching Rust

## Risk Mitigation

### Risk: Behavioral Changes
**Mitigation**:
- Feature flags for each migrated prompt
- A/B testing harness comparing old vs new
- Gradual rollout (researcher first, then conductor)

### Risk: Performance Overhead
**Mitigation**:
- Measure BAML client call latency (expected <1ms)
- Cache compiled prompts
- Fallback to inline if needed (emergency only)

### Risk: Template Rendering Differences
**Mitigation**:
- Snapshot tests for prompt outputs
- Compare byte-for-byte with original
- Document any intentional changes

## Related Documents

- `adr-0002-rust-nix-build-and-cache-strategy.md` - BAML build context
- `simplified-agent-harness.md` - Agent harness patterns
- `model-provider-agnostic-runbook.md` - Model flexibility
