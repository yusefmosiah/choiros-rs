# Operational Concepts Pruning Catalog

Date: 2026-02-14  
Status: Working catalog (authoritative for simplification discussions)  
Scope: Keep/simplify/remove decisions for runtime and product language

## Narrative Summary (1-minute read)

ChoirOS is reducing conceptual surface area.
The goal is fewer abstractions, clearer authority, and less accidental architecture.

This catalog tracks each operational concept with one action:
`keep`, `simplify`, `defer`, `remove`, or `archive`.

Current direction emphasizes:
1. living-document human interface,
2. conductor-centered non-blocking orchestration,
3. minimal typed request primitives,
4. deterministic safety rails only.

## What Changed

1. Added a concrete pruning catalog for operational terminology.
2. Marked "escalation subsystem" as removed in favor of `request` message v0.
3. Marked standalone chat app framing as removed from active architecture.
4. Marked watcher/wake as de-scoped from normal run-step authority.

## What To Do Next

1. Review this catalog at start of architecture changes.
2. Require new concepts to justify why existing concepts are insufficient.
3. Move retired terms to archive-only docs.
4. Add implementation references as each concept decision lands in code/tests.

---

## Catalog

1. `Living Document Human Interface`  
   - Decision: `keep`  
   - Reason: Durable human interaction model aligned with product direction.  
   - Action: Keep as primary human surface in all active docs.

2. `Conductor as Control Plane`  
   - Decision: `keep`  
   - Reason: Clear orchestration authority across workers and app agents.  
   - Action: Keep central in runtime and prompt context policy.

3. `Logical Subagents via Actor Messaging`  
   - Decision: `keep`  
   - Reason: Preserves hierarchy without blocking API constraints.  
   - Action: Enforce no-poll/no-block turn invariants.

4. `Agent Tree Snapshot`  
   - Decision: `keep`  
   - Reason: Required bounded wake context for model-led replanning.  
   - Action: Implement typed snapshot contract and determinism tests.

5. `Escalation Subsystem`  
   - Decision: `remove`  
   - Reason: Premature abstraction; replaced by minimal `request` message kind.  
   - Action: Use `request` terminology in active contracts/docs.

6. `Conductor Request Message v0`  
   - Decision: `keep`  
   - Reason: Minimal non-blocking primitive for decision/attention asks.  
   - Action: Implement with required fields only; defer advanced controls.

7. `Watcher as Run-Step Authority`  
   - Decision: `remove`  
   - Reason: Competes with conductor authority and adds hidden loops.  
   - Action: Keep watcher only as optional recurring-event detector.

8. `Wake Loop Polling`  
   - Decision: `remove`  
   - Reason: Violates interruptibility and concurrency goals.  
   - Action: Event-driven wakes only.

9. `Standalone Chat App as Primary Interface`  
   - Decision: `remove`  
   - Reason: Conflicts with living-document-first human interaction strategy.  
   - Action: Keep only in historical/archived context.

10. `Prompt Phrase Matching as Workflow Control`  
    - Decision: `remove`  
    - Reason: Brittle and non-auditable orchestration authority.  
    - Action: Typed metadata only for control authority.

11. `Deterministic Workflow Branches for Multi-Step Planning`  
    - Decision: `remove`  
    - Reason: Conflicts with model-led control-flow policy.  
    - Action: Retain deterministic logic only in safety/operability rails.

12. `Tracing App-Agent Harness`  
    - Decision: `defer`  
    - Reason: Must follow rollout order (human UX -> API -> harness).  
    - Action: Build phases in order; do not skip to harness first.

13. `Adapter (broad worker abstraction)`  
    - Decision: `simplify`  
    - Reason: Current trait mixes execution, reporting, and plumbing concerns.  
    - Action: Keep boundary but narrow to execution-focused `worker_port` intent.

14. `Harness phase vocabulary (decide/execute/loop/finish as separate abstractions)`  
    - Decision: `simplify`  
    - Reason: Creates conceptual overhead for what is one runtime while loop.  
    - Action: Treat as one loop with typed actions; avoid introducing phase frameworks.

15. `Worker Signal Contract (separate subsystem concept)`  
    - Decision: `remove`  
    - Reason: Adds legacy terminology and duplicate signaling semantics.  
    - Action: Use canonical worker event model (`progress/result/failed/request`) with live document updates.

## Governance Rule

Any new operational concept must include:
1. problem statement,
2. why existing concepts fail,
3. keep/simplify/remove impact,
4. rollback/retirement conditions.

If this cannot be stated clearly, do not add the concept.
