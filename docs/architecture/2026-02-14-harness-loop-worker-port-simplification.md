# Harness Loop and Worker Port Simplification

Date: 2026-02-14  
Status: Active simplification direction (authoritative)  
Scope: Worker harness loop shape, `adapter` concept reduction, runtime naming clarity

## Narrative Summary (1-minute read)

Yes: the harness should be thought of as a simple while loop.

`Decide -> Execute -> loop/finish` is only a conceptual explanation,
not a requirement to keep separate phase abstractions.

Also, `adapter` is currently doing too much and the name is overloaded.
We keep the boundary, but simplify and rename it to `worker_port`:
a thin worker-specific execution port used by a shared harness loop.

## What Changed

1. Declared the harness as a single loop runtime model (not phase-object architecture).
2. Declared `adapter` terminology to be simplified to `worker_port` in active docs/contracts.
3. Reduced worker boundary intent to execution-focused responsibilities.
4. Kept orchestration authority out of worker boundary design.

## What To Do Next

1. Update active docs to prefer `worker_port` over broad `adapter` language.
2. Keep harness loop represented as one while loop with typed decision actions.
3. Move progress/report event plumbing into shared harness services where possible.
4. Defer extra hooks until a concrete worker proves they are required.

---

## Current State (What Exists)

Today, the harness trait called `AgentAdapter` mixes several concerns:
1. worker role and prompt/tool description,
2. tool execution,
3. progress emission,
4. worker report emission,
5. optional defer hints.

This works, but it makes the abstraction feel heavier than needed.

## Simplified Runtime Mental Model

Use this as the canonical harness behavior:

```text
while step < max_steps and status == running:
  decision = decide(messages, worker_spec, context)
  match decision.action:
    tool_call -> execute tool(s), append results to messages, continue
    complete  -> return completed
    block     -> return blocked

if still running after max_steps:
  return incomplete
```

This is the runtime.
No extra phase framework is required.

## Worker Port (`worker_port`) v0

Keep only the boundary needed for worker-specific behavior:
1. worker identity/spec (`role`, tool schema, static constraints).
2. tool execution (`execute_tool_call`).

Shared harness services should own:
1. loop control and step budgeting,
2. trace/progress event emission,
3. final status/report envelope assembly.

Optional hooks remain optional and evidence-driven.

## Naming Decision

In active architecture language:
1. prefer `worker_port` for worker execution boundary,
2. reserve `adapter` for historical references and existing code identifiers,
3. avoid using `adapter` for conductor RPC wrappers.

## Guardrails

1. Worker port does not orchestrate other workers.
2. Worker port does not own conductor policy.
3. Worker port does not add polling or blocking loops.
4. Conductor remains orchestration authority.

## Acceptance Signals

1. Docs describe harness runtime as one simple while loop.
2. New architecture docs use `worker_port` terminology.
3. Worker boundaries are execution-focused and easier to explain.
4. No new phase abstractions are introduced without hard evidence.
