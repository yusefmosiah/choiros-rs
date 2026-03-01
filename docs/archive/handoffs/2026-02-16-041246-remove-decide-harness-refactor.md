# Handoff: Remove Decide Step From Harness (Tool-Loop Refactor)

## Session Metadata
- Created: 2026-02-16 04:12:46
- Project: /Users/wiz/choiros-rs
- Branch: main
- Session duration: ~1.5 hours

### Recent Commits (for context)
  - 9883837 Improve tracing graph UI
  - bed370b Fix writer panic on dropped signal
  - 966c001 Fix writer panic from dropped signal
  - f7678d4 Condense prompt bar UI
  - 7f9d9c7 Update mobile viewport meta

## Handoff Chain

- **Continues from**: [2026-02-14-packet-e-tracing-foundation-assessment.md](./2026-02-14-packet-e-tracing-foundation-assessment.md)
  - Previous title: Packet E - Tracing Foundation Assessment
- **Supersedes**: None

> Previous handoff is related context; this handoff is focused on BAML/tool-loop contract refactor work.

## Current State Summary

Refactor work started to fix null-heavy tool call payloads and brittle parsing in the agent loop. Completed: switched to discriminated tool-call unions, removed old flat compatibility parsing in runtime adapters, regenerated BAML client, and validated build/tests. Then started a second simplification pass to reduce contract verbosity (`summary/reason` -> `message`, slimmer tool arg fields, `ctx.output_format(...)` customization), but that second pass was interrupted before regeneration/runtime alignment. Current repo state intentionally contains a mismatch: `baml_src/types.baml` has the new minimal contract, while generated Rust client code still reflects the prior union contract with `summary/reason`.

## Codebase Understanding

### Architecture Overview

- Current harness model is `Decide -> Execute tool(s) -> Decide ...`, with `Action::{ToolCall,Complete,Block}`.
- The main fragility is not only tool args shape; it is that large tool outputs are injected back into subsequent Decide context, causing prompt bloat and occasional BAML parse failure (`missing action` in streamed parse windows).
- Runtime adapter integration is cleanly centralized through `WorkerPort::execute_tool_call`, so removing the explicit Decide classifier should be done in harness loop + BAML output contract, not inside each adapter’s tool logic.
- BAML-generated files are committed and consumed directly; source changes in `baml_src/*.baml` do nothing until `baml-cli generate` is run.

### Critical Files

| File | Purpose | Relevance |
|------|---------|-----------|
| baml_src/types.baml | BAML decision/tool-call schema | Primary target for contract refactor |
| baml_src/agent.baml | Decide prompt + output_format rendering | Controls schema verbosity and parsing reliability |
| sandbox/src/actors/agent_harness/mod.rs | Main agentic loop (`Decide` + tool execution) | Core code to remove/replace Decide step |
| sandbox/src/actors/researcher/adapter.rs | Researcher tool execution mapping | Updated to strict tool-call union handling |
| sandbox/src/actors/terminal.rs | Terminal tool execution mapping | Updated to strict `bash` union variant handling |
| sandbox/src/baml_client/* | Generated BAML Rust client artifacts | Must be regenerated and kept in sync |

### Key Patterns Discovered

- Use discriminated tool unions with literal `tool_name` values; avoid giant optional args structs.
- Keep worker execution deterministic in Rust; keep orchestration flexible in model contract.
- Test workflow preference in this repo: targeted test binaries/wrappers over broad test sweeps (`./scripts/sandbox-test.sh --lib ...`).
- The codebase tolerates dirty trees; do not revert unrelated user changes.

## Work Completed

### Tasks Finished

- [x] Refactored BAML tool-call schema to discriminated unions and removed legacy flat compatibility fields.
- [x] Ran `baml-cli generate` and updated generated client artifacts.
- [x] Updated harness + adapters to consume union variants (no fallback field parsing).
- [x] Updated `message_writer` contract from `old_text/new_text` mode hacks to `mode/mode_arg`.
- [x] Ran `cargo fmt`, `cargo check -p sandbox`, and targeted lib tests (`agent_harness`, `researcher`, `terminal`) successfully.
- [x] Began second-pass schema simplification to reduce structural overhead in `Decide` output.

### Files Modified

| File | Changes | Rationale |
|------|---------|-----------|
| baml_src/types.baml | Tool-call unions introduced; then second-pass simplification to minimal args + `message` field | Reduce null-heavy payloads and contract bloat |
| baml_src/agent.baml | Added stricter no-null guidance and customized `ctx.output_format(...)` in second pass | Reduce verbose schema rendering and placeholder nulls |
| sandbox/src/actors/agent_harness/mod.rs | Union tool-call handling helpers; trace payload extraction updated | Remove old `tool_args` flat parsing assumptions |
| sandbox/src/actors/researcher/adapter.rs | Strict variant matching for all tools; `message_writer` now uses `mode/mode_arg` | Remove backward compatibility and simplify semantics |
| sandbox/src/actors/terminal.rs | Strict `BashToolCall` handling only | Remove old `cmd/command` fallback behavior |
| sandbox/src/baml_client/* (multiple files) | Regenerated from BAML (first-pass union refactor) | Keep runtime types synced with schema |

### Decisions Made

| Decision | Options Considered | Rationale |
|----------|-------------------|-----------|
| Remove backward compatibility in tool args | Keep legacy flat fields vs delete | User requested pre-MVP cleanup; less ambiguity and fewer nulls |
| Use discriminated unions for tool calls | Giant optional object vs per-tool union classes | Better parse reliability and cleaner model outputs |
| Move `message_writer` mode semantics into explicit fields | Keep overloading `old_text/new_text` vs `mode/mode_arg` | Reduces semantic confusion and accidental invalid payloads |
| Start second pass to collapse decision shape | Keep `summary/reason` optional fields vs required single `message` | Reduce nullable boilerplate in model output |

## Pending Work

## Immediate Next Steps

1. Run `baml-cli generate` to sync generated Rust client code with the latest `baml_src/types.baml` and `baml_src/agent.baml`.
2. Reconcile compile/runtime changes from `AgentDecision` field changes (`summary/reason` -> `message`) across harness logic and traces.
3. Refactor the harness loop to remove explicit Decide classifier semantics and use a direct tool-calling loop contract.

### Immediate Next Steps

1. Finish the second pass cleanly: regenerate BAML (`baml-cli generate`) against current `baml_src/types.baml` and reconcile all Rust compile errors from `AgentDecision` (`message` replacing `summary/reason`) and slimmed tool args.
2. Implement harness refactor to remove explicit Decide classifier semantics: convert to tool-loop contract where model returns tool calls (or final message when no calls), then update loop termination logic accordingly.
3. Add prompt/context protections in harness: bounded tool-result echo into next model turn (or artifact handles/chunked reads) to prevent token blowups and parse failures.

### Blockers/Open Questions

- [ ] Decide whether to keep `Action` enum at all or replace with implicit completion (`tool_calls.is_empty() => final`) in BAML schema.
- [ ] Confirm BAML best-practice target contract shape for loop-only tool calling (single turn output type) before editing harness control flow.
- [ ] Determine if `fetch_url` should normalize GitHub blob URLs to raw content before returning to model.

### Deferred Items

- Full removal of `Decide` step was deferred after user requested explicit handoff to continue in a fresh session.

## Context for Resuming Agent

## Important Context

The repository currently has a deliberate source/generated mismatch: BAML source schema was simplified further, but generated client files were not regenerated after those last edits. Because of that, current `cargo check` success reflects older generated types, not the latest source contract. The next agent must start by regenerating BAML and then making runtime changes against the newly generated types before attempting the Decide-removal harness refactor.

### Important Context

State is split across two refactor phases:
1) Completed/stable phase: unionized tool calls + runtime adapter updates + generated code in sync + passing checks/tests.
2) In-progress phase: `baml_src/types.baml` and `baml_src/agent.baml` were further simplified (single `message` field, slimmer args, custom `ctx.output_format`) but `baml-cli generate` has NOT been run after those latest edits.  
This means current compile/test success reflects old generated client contract, not latest source schema.  
Before making any harness logic changes, run generation and reconcile the contract intentionally. Do not assume current generated `AgentDecision` matches `baml_src/types.baml`.

### Assumptions Made

- Assumed pre-MVP allows deleting all backward compatibility layers.
- Assumed current parse failures are amplified by oversized tool-output context and nullable-heavy response shape.
- Assumed harness/refactor should prioritize determinism in runtime and minimal schema burden on model outputs.

### Potential Gotchas

- `cargo check` can pass while BAML source and generated files are out of sync.
- Generated union type alias names are long (`Union7...`) and noisy; avoid hand-editing generated artifacts.
- There is an unrelated untracked file: `docs/architecture/browser-wasm-vector-architecture.md` (do not modify/revert unless asked).
- `Action` enum comments in `baml_src/types.baml` still reference old `summary/reason` wording; cleanup needed once final contract is chosen.

## Environment State

### Tools/Services Used

- `baml-cli 0.217.0` for generation.
- `cargo fmt`, `cargo check -p sandbox`.
- Targeted tests via `./scripts/sandbox-test.sh --lib <name>`.

### Active Processes

- None known running from this session.

### Environment Variables

- `CHOIR_TERMINAL_ALLOWED_COMMAND_PREFIXES`
- Standard model/provider keys used by runtime (not inspected in this session)

## Related Resources

- `/Users/wiz/choiros-rs/baml_src/types.baml`
- `/Users/wiz/choiros-rs/baml_src/agent.baml`
- `/Users/wiz/choiros-rs/sandbox/src/actors/agent_harness/mod.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/researcher/adapter.rs`
- `/Users/wiz/choiros-rs/sandbox/src/actors/terminal.rs`
- [BAML types/unions docs](https://docs.boundaryml.com/ref/baml/types)
- [BAML `ctx.output_format` docs](https://docs.boundaryml.com/ref/prompt-syntax/ctx-output-format)

---

**Security Reminder**: Before finalizing, run `validate_handoff.py` to check for accidental secret exposure.
