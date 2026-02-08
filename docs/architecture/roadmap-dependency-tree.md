# ChoirOS Roadmap Dependency Tree

**Date:** 2026-02-08
**Status:** Working roadmap (execution-oriented)

---

## Execution Snapshot (2026-02-08)

- Phase B is actively implemented:
  - delegated terminal task contracts (`task_id`, `correlation_id`, scope keys, status) are live
  - asynchronous delegation API is live in `ApplicationSupervisor`
  - delegated tasks persist worker lifecycle events (`started/progress/completed/failed`)
  - terminal delegation routes through internal agentic harness in `TerminalActor`

- Phase F (Identity/Scope) remains in progress:
  - scope-aware chat/thread isolation implemented for API + websocket/event retrieval
  - full non-chat scope propagation remains open

- **New Architecture Direction** (2026-02-08):
  - Capability Actor pattern: tool → agent → actor with standard contract
  - StateIndexActor: compressed state plane (AHDB) for all actor context
  - PromptBarActor: universal entrypoint above individual apps
  - ResearcherActor: web research capability actor abstracted as `web_search` tool
  - Safety as capability: Verifier, HITL (email), Policy, LLM guardrails
  - Self-hosting introspection: Choir modifying Choir with headless test sandboxes
  - Directives app as first-class control surface (persistent directive forest + event links)
  - Policy pattern split:
    - supervision for lifecycle only
    - deterministic local policy checks for normal actor calls
    - policy actors for high-risk escalation paths only

---

## Purpose

Define a safe execution order for:
1. Multiagent control plane and capability architecture
2. Safety and verification infrastructure
3. Context management and state plane
4. Auth and identity enforcement
5. Sandbox persistence and hypervisor integration
6. Self-hosting introspection (code modification)
7. Nix/NixOS migration

Biased toward delivery safety, low rework, and solid foundations before self-hosting features.

---

## Dependency Tree

```text
A. Supervision Cutover (complete)
   |
   +--> B. Multiagent Control Plane v1
   |      |
   |      +--> B1. Capability Contract Schema (docs + types)
   |      |      |
   |      |      +--> B2. StateIndexActor (compressed state plane)
   |      |             |
   |      |             +--> B3. PromptBarActor (universal entrypoint)
   |      |                    |
   |      |                    +--> B4. First Capability Pair: GitActor + ResearcherActor
   |      |                           |
   |      |                           +--> B5. SafetyOrchestrator (verifiers, policy)
   |      |                                  |
   |      +--> F. Identity and Scope Enforcement v1 (parallel with B2-B5)
   |             |
   |             +--> C. Chat Delegation Refactor (depends on B3, F)
   |                    |
   |                    +--> D. Context Broker v1 (depends on B2, F)
   |                           |
   |                           +--> E. App Expansion Wave 1
   |
   +--> G. SandboxFS Persistence (parallel with D)
          |
          +--> H. Hypervisor Integration
                 |
                 +--> J. Self-Hosting Introspection v1
                        |
                        +--> J1. Prompt visibility + editing
                        +--> J2. Code introspection (read-only)
                        +--> J3. Headless test sandbox
                        +--> J4. Safe self-modification loop
   |
   +--> I. Nix/NixOS Migration (cross-cutting)
          +--> I1. Dev shell + toolchain pinning
          +--> I2. CI parity and reproducible builds
          +--> I3. Deployment packaging
          +--> I4. Host-level NixOS operations
```

---

## Critical Path (Updated)

1. **B1** Capability Contract Schema — standard envelope for all actors
2. **B2** StateIndexActor — compressed state plane (foundation for context)
3. **F** Identity and Scope Enforcement — security boundary
4. **B3** PromptBarActor — universal entrypoint
5. **B4** GitActor + ResearcherActor — prove capability pattern on local+network tasks
6. **B5** SafetyOrchestrator — verifiers and policy
7. **C** Chat Delegation Refactor — router to capabilities
8. **D** Context Broker v1 — layered retrieval
9. **G+H** Persistence + Hypervisor — foundation for J
10. **J** Self-Hosting Introspection — Choir modifying Choir

---

## Execution Phases (Detailed)

### Phase B1 - Capability Contract Schema
**Objective:** Define the standard interface all capability actors implement.

**Deliverables:**
- `shared-types/src/capability.rs`: `CapabilityInput`, `CapabilityOutput`, `CapabilityEvent`
- `shared-types/src/safety.rs`: `SafetyPolicy`, `VerificationLevel`, `EnforcementLevel`
- `docs/design/capability-lifecycle.md`: State machine specification
- `docs/design/safety-decision-flow.md`: Safety architecture

**Gate:**
- Schema review complete, types compile, documentation approved.

---

### Phase B2 - StateIndexActor (Compressed State Plane)
**Objective:** Build the AHDB state index that all actors query for context.

**Deliverables:**
- `StateIndexActor`: subscribes to EventBus, maintains compressed snapshots
- `CompressedState` struct: active goals, directives summary, recent blocks, scope anchors
- `DirectiveForest`: hierarchical directive tree with live status
- Query API: `GetLatestSnapshot`, `GetTaskTree`, `GetAlerts`

**Gate:**
- Snapshot generation < 100ms for 10K events.
- Task tree renders in UI with live updates.

---

### Phase F - Identity and Scope Enforcement v1
**Objective:** Prevent cross-user/session data leakage.

**Deliverables:**
- Required scope keys: `user_id`, `session_id`, `app_id`, `thread_id`, `sandbox_id`
- Enforcement at API and supervisor boundaries
- Auth integration: session validation, token lifecycle

**Gate:**
- Isolation tests prove no cross-user/session retrieval leakage.

---

### Phase B3 - PromptBarActor (Universal Entrypoint)
**Objective:** Global intent router above individual apps.

**Deliverables:**
- `PromptBarActor`: receives all user input (typed, voice)
- Intent classification: NL → structured capability calls
- Routing to: ChatActor, TerminalActor, GitActor, etc.
- UI: persistent input bar, Directives app with hierarchical directive display

**Gate:**
- "Open chat with X" and "Run command Y" both route correctly.
- Task tree visible and updates live.

---

### Phase B4 - GitActor (First Capability)
**Objective:** Prove capability actor pattern with concrete local+network implementations.

**Deliverables:**
- `GitActor`: typed git ops (status, diff, branch, commit, push, log, checkout)
- `ResearcherActor`: typed web research ops (search, fetch, extract, synthesize)
- Tool abstraction:
  - `bash` -> TerminalActor
  - `web_search` -> ResearcherActor
- Event-sourced: `EVENT_GIT_COMMIT`, `EVENT_GIT_BRANCH`, etc.
- Event-sourced researcher lifecycle:
  - `research.planning`
  - `research.search_results`
  - `research.fetch_started|completed`
  - `research.synthesis_started|completed`
- Safety policy integration
- Observable via same `CapabilityEvent` schema as all capabilities

**Gate:**
- Git operations complete with full event trail.
- Web research completes with citations and observable actor-call timeline.
- Failed operations retry with supervision.

---

## Immediate Build Order (Clarity Pass)

0. **Directives app lock**: make hierarchical directives the primary operator view.
1. **B1 contract lock**: finalize shared capability/task/event schema.
2. **B2 StateIndexActor skeleton**: compressed snapshot + directive tree query API.
3. **B3 PromptBarActor skeleton**: route one universal intent (`open chat with prompt`).
4. **B4 ResearcherActor v1**: implement as `web_search` capability with streamed actor-call phases.
5. **B4 GitActor v1**: typed git operations through same capability contract.
6. **Watcher bootstrap**: emit blockers/stalls/failures into StateIndex directive tree.

Rule:
- Prioritize concrete capability actors (`ResearcherActor`, `GitActor`) over broad app expansion.
- Do not ship further app complexity until Directives app + hard capability boundaries are in place.

---

### Phase B5 - SafetyOrchestrator
**Objective:** Safety as a capability layer, not an afterthought.

Policy execution model:
- Do not route all actor calls through a policy/supervisor bottleneck.
- Use direct actor-to-actor calls with deterministic local policy checks.
- Route only high-risk actions to policy actor workflows.

**Deliverables:**
- `PolicyEnforcementActor`: static rule checking
- `VerifierActor`: automatic verification (code, claims)
- `LLMGuardrailActor`: prompt-based safety checks
- `HumanInTheLoopActor`: email-based confirmation (Resend + mymx)
- Safety decision flow: Policy → Verifier → HITL

**Gate:**
- Policy blocks unauthorized capability calls.
- Verifier catches bad code/claims.
- HITL sends email and blocks until response.

---

### Phase C - Chat Delegation Refactor v1
**Objective:** Chat is planner/router; capabilities do the work.

**Deliverables:**
- Chat routes intents to capability actors via PromptBar
- Retains narrow direct-tool fallback for latency-critical paths
- Timeout/retry/error contracts for all capability calls
- Observable delegation flow

**Gate:**
- Chat delegates to Terminal via capability contract.
- Graceful degradation on capability failure.

---

### Phase D - Context Broker v1
**Objective:** Layered context retrieval with drill-down.

**Deliverables:**
- Canonical events in libsql
- Memory layers: global, workspace, session, thread
- API: `brief_context` + `relevant_handles` + `expand(handle)`
- Integration with StateIndexActor snapshots

**Gate:**
- Relevance test: prior session insights improve later task.
- No scope boundary violations.

---

### Phase G - SandboxFS Persistence
**Objective:** Durable virtual filesystem.

**Deliverables:**
- SandboxFS interface: read/write/list/delete/snapshot/rehydrate
- SQLite-backed storage
- Versioned snapshots

**Gate:**
- Restart/rehydrate test restores files deterministically.

---

### Phase H - Hypervisor Integration
**Objective:** Bind sandbox lifecycle to identity and state.

**Deliverables:**
- Session-attached sandbox allocation
- Attach/detach/restore lifecycle
- Integration with SandboxFS snapshots

**Gate:**
- Multi-session integration tests pass with strict isolation.

---

### Phase J - Self-Hosting Introspection v1
**Objective:** Choir can see and modify itself safely.

**J1. Prompt Visibility + Editing**
- `PromptRegistry`: all system prompts visible in UI
- `PromptEditorApp`: view, edit, version prompts
- Prompt safety guardrails (LLM-based checks)

**J2. Code Introspection**
- `CodeIntrospectionActor`: read source, AST index, find implementations
- `SystemBrowserApp`: browse actors, see source, trace events

**J3. Headless Test Sandbox**
- `HeadlessSandboxActor`: spawn isolated Choir instance
- Run tests against modified code
- Nix reproducible builds (I2)

**J4. Safe Self-Modification Loop**
- Change proposal → safety checks → headless test → HITL approval
- Browser state hydration for live cutover
- Rollback capability

**Gate:**
- Choir modifies a prompt, tests pass, deploys safely.

---

### Phase I - Nix/NixOS Migration (Cross-Cutting)

**I1. Dev Shell + Toolchain Pinning**
- `flake.nix` with Rust, Node, dependencies
- `direnv` integration

**I2. CI Parity and Reproducible Builds**
- GitHub Actions use Nix
- Binary cache

**I3. Deployment Packaging**
- Docker image from Nix build
- EC2/NixOS deployment

**I4. Host-Level NixOS Operations**
- Full NixOS host configuration
- Declarative infrastructure

---

## App Expansion Strategy

**Wave 1 (after D):**
- File Explorer
- Settings
- System Browser (code introspection)
- Prompt Editor
- Multimedia and generic viewers
- Safe iframe window

**Wave 2 (after J3):**
- Mail App (HITL + email ingress)
- Calendar
- Advanced media workflows

**Rule:** No new app requiring persistent per-user data until F is complete.

---

## Now vs Later (Updated)

**Now (must execute before capability rollout):**
- B1 Capability Contract Schema
- B2 StateIndexActor (compressed state plane)
- F Identity and Scope Enforcement
- B3 PromptBarActor
- B4 GitActor (prove pattern)
- B5 SafetyOrchestrator (policy, verifier, basic HITL)

**Next (stable capability baseline):**
- C Chat Delegation Refactor
- D Context Broker v1
- E App Expansion Wave 1

**Later (after G+H foundation):**
- G SandboxFS Persistence
- H Hypervisor Integration
- J Self-Hosting Introspection
- E App Expansion Wave 2
- I3/I4 deeper Nix/NixOS

---

## Risk Register (Updated)

**Risk:** Capability contract instability causes rework across actors.
- **Mitigation:** B1 is docs-only gate; no code until schema locked.

**Risk:** StateIndexActor becomes bottleneck.
- **Mitigation:** Compressed snapshots only; full state fetched on demand.

**Risk:** Safety guardrails are bypassable.
- **Mitigation:** SafetyOrchestrator is capability all others call; no direct tool access.

**Risk:** HITL email delivery unreliable.
- **Mitigation:** Multiple channel support (deferred); fallback to in-app notifications.

**Risk:** Self-modification loop is insecure.
- **Mitigation:** Headless sandbox + HITL + rollback; no auto-deploy without human.

**Risk:** Context leakage across users/sessions.
- **Mitigation:** F must land before D or J.

**Risk:** Documentation drift.
- **Mitigation:** Handoff update after each phase gate.

---

## Definition of Ready for Capability Rollout

- B1 schema locked and documented
- B2 StateIndexActor serving snapshots
- F scope enforcement active
- B3 PromptBarActor routing intents
- One capability (B4 GitActor) proven end-to-end
- SafetyOrchestrator blocking unauthorized calls

## Definition of Done for Pre-Introspection Foundation

- Phases B1-B5, F, C, D complete
- No known cross-scope leaks
- Capability workflows observable and recoverable
- Clear plan for G, H, J with test gates

## Definition of Ready for Self-Hosting

- G SandboxFS Persistence complete
- H Hypervisor Integration complete
- J1, J2 complete (prompt + code visibility)
- J3 headless sandbox proven
- Nix I2 reproducible builds
- Rollback capability tested

---

## References

- `docs/design/2026-02-08-capability-actor-architecture.md`
- `docs/design/2026-02-08-self-hosting-introspection.md`
- `docs/dev-blog/from-slop-to-signal-verified.md` (Ralph Loop demo)
