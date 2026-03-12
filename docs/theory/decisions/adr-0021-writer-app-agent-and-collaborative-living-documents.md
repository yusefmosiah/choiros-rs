# ADR-0021: Writer App Agent and Collaborative Living Documents

Date: 2026-03-10
Kind: Decision
Status: Draft
Priority: 2
Requires: [ADR-0001, ADR-0007]
Supersedes: []
Authors: wiz + Codex

## Narrative Summary (1-minute read)

Writer is the first true app agent in ChoirOS.

This matters because Writer is not just "the writing UI." It is the first proof
that ChoirOS should be built around app agents with durable product surfaces,
not around chat as the primary AI interaction model. The living document is the
canonical human-facing artifact. Versions tell the story of a run. Workers such
as Terminal and Researcher support that artifact instead of replacing it with a
scrolling transcript.

This ADR makes four decisions.

1. Writer is the first app agent and the reference model for future app agents.
2. The collaborative living document is the canonical authored surface for
   Writer.
3. App agents orchestrate within their domain; Conductor remains global
   orchestration and policy authority; workers remain reusable primitives.
4. Cross-agent orchestration must be explicit and typed. The semantics should be
   valid in both `ractor` space and Go-style service space, even if the runtime
   mechanics differ.

This ADR deliberately does not design every future app. It defines the
architecture needed to focus Writer, make the next app-agent class legible, and
prevent ad hoc orchestration growth.

## What Changed

- Declares Writer the first canonical app agent.
- Defines the collaborative living document model as Writer's core product
  surface.
- Separates app agents from worker primitives.
- Defines a default hierarchy for orchestration and a bounded rule for
  cross-agent calls.
- Narrows the future-app discussion so not every window becomes an app agent by
  default.

## What To Do Next

1. Harden the Writer contract around versions, marginalia, artifacts, run
   status, and commit authority.
2. Define the typed request envelope for app-agent to worker and app-agent to
   app-agent calls.
3. Keep Terminal and Researcher as worker primitives while Writer proves the app
   agent model.
4. Treat Browser as the most likely next app agent after Writer.
5. Defer simple display apps such as image, audio, and video viewers unless
   they need their own orchestration authority.

## Context

### The product problem

Chat-first AI interfaces are not the target direction for ChoirOS.

ChoirOS is trying to build durable, inspectable, replayable work surfaces:

- living documents,
- coding runs,
- research trajectories,
- traces and dashboards,
- later browser and media-oriented surfaces.

Writer is the first serious attempt at that model. It has already expanded from
research-oriented output toward coding, scripting, and orchestration of coding
agents. That makes Writer more than a text editor. It is the first proof of a
non-chat app-agent architecture.

### The architecture problem

The runtime now has multiple kinds of intelligence-bearing components:

- Conductor,
- Writer,
- Terminal,
- Researcher,
- future Browser,
- future Tracing app,
- future specialized surfaces.

Without an explicit model, the system will drift toward one of two failure
modes:

- everything routes through Conductor and Conductor becomes overpowered,
- or every agent can call every other agent arbitrarily and the runtime becomes
  an unreadable mesh.

We need a model that preserves delegation and expressive power without losing
clarity.

## Decision

### 1. Writer is the first app agent

Writer is the first canonical app agent in ChoirOS.

That means Writer is:

- a product surface,
- a durable authored state machine,
- a domain orchestrator,
- a reference implementation for future app-agent design.

Writer is not just a frontend window. It is the runtime responsible for
turning prompts, edits, worker outputs, and collaboration signals into a
coherent living document.

### 2. Writer is built around a collaborative living document

The living document is the canonical surface for Writer.

Required properties:

- it is durable,
- it has immutable versions,
- versions tell the story of the run,
- it supports user, Writer, and later collaborator authorship,
- it keeps canonical content separate from marginalia and artifacts,
- it is readable without replaying an opaque transcript.

The first user prompt is a real version, not throwaway scaffolding.

Writer is therefore not a "chat that happens to save markdown." It is a
document-native collaborative system whose runtime happens to use AI.

### 3. Writer is the first proof of AI that is not chat-based

This is a product decision, not just a UI preference.

Writer proves that:

- AI work can happen around durable artifacts rather than transient turns,
- orchestration can serve a document rather than a transcript,
- a user can work with a choir of workers without that choir becoming the
  primary human-visible interface.

This is strategically important because later app agents should build on the
same premise instead of falling back to "another chat tab with tools."

### 4. App agents and worker primitives are different classes

ChoirOS should distinguish between app agents and worker primitives.

#### App agents

App agents own a domain surface and domain state.

Examples:

- Writer
- future Browser app
- future Tracing app

App agents may:

- accept user prompts directly,
- hold durable domain state,
- orchestrate worker primitives,
- render domain-specific output,
- expose a typed API to other app agents or Conductor.

#### Worker primitives

Workers are reusable execution capabilities, not primary product surfaces.

Current examples:

- Terminal
- Researcher
- Memory retrieval

Workers may:

- execute bounded jobs,
- emit progress and results,
- maintain local execution state as needed,
- serve multiple app agents over time.

Workers should not become accidental top-level product surfaces merely because
they are powerful.

### 5. Not every app window is an app agent

This is the focus rule.

Some app surfaces may just be viewers or specialized utilities:

- image display,
- audio playback,
- video playback,
- simple document viewers.

Those do not automatically deserve app-agent status.

They become app agents only if they need:

- prompt intake,
- durable domain state,
- domain orchestration,
- worker delegation,
- or a typed API other agents can call.

This keeps the architecture focused. Browser and Tracing are plausible future
app agents. Plain media viewers are not, unless their domain grows beyond
display.

### 6. Hierarchical orchestration is the default model

The default runtime hierarchy is:

```text
Human
  -> Conductor
     -> App Agent
        -> Worker Primitive
```

This means:

- Conductor orchestrates globally,
- app agents orchestrate within their domain,
- workers execute domain-independent primitives.

Conductor is orchestration-only, but not the only orchestrator.

Writer and future app agents are allowed to orchestrate, but only inside bounded
domain authority.

### 7. Cross-agent orchestration must be explicit and typed

Cross-agent calls are allowed, but they must not become an unbounded peer mesh.

Default rule:

- app agent to worker calls are normal,
- app agent to app agent calls are exceptional but supported,
- worker to worker orchestration should stay narrow and task-specific,
- worker to app-agent escalation should usually flow through the owning app
  agent or Conductor rather than creating hidden control loops.

Cross-agent calls should use typed envelopes with:

- requester identity,
- target identity,
- objective,
- capability or lease metadata,
- correlation ids,
- timeout and cancellation,
- result contract,
- durable event emission.

This gives the system the expressive power to support future cases such as:

- Writer asking Browser for a scripted page interaction,
- Tracing asking Writer for run summaries,
- Terminal requesting bounded research help,
- Browser exposing domain actions to Writer or Conductor.

But it does so without endorsing unconstrained "everyone messages everyone"
architecture.

### 8. Conductor remains the global policy and routing authority

Conductor should not do all orchestration work, but it should remain the global
authority for:

- policy,
- budgets,
- capability grants,
- priority,
- cancellation,
- run startup,
- cross-domain routing when needed.

This prevents app agents from quietly acquiring platform-level power.

### 9. Ractor space and Go space should preserve the same semantics

The architecture should not depend on `ractor` as the meaning of the system.

#### In `ractor` space

The model looks like:

- ConductorActor
- WriterActor
- BrowserActor later
- TerminalActor
- ResearcherActor
- typed actor messages
- supervisor-managed restart and lookup

This is acceptable as an implementation.

#### In Go-style service space

The same model would look like:

- Conductor service
- Writer service
- Browser service later
- Terminal runtime service
- Researcher service
- typed request structs,
- job managers,
- explicit eventing,
- explicit restart and recovery logic.

The important point is that the semantics are the same:

- domain ownership,
- typed delegation,
- durable ids,
- explicit cancellation,
- event-backed observability,
- bounded authority.

Thinking about both spaces clarifies the real architectural question:
the key design problem is not actor framework choice. It is how authority,
ownership, and delegation are partitioned.

### 10. Browser is the most likely next app agent

After Writer, the most plausible next app agent is Browser.

Browser qualifies because it would:

- own a visible domain surface,
- accept prompts,
- script its own actions,
- maintain state beyond one tool call,
- expose typed domain actions to other agents.

This is materially different from "just an iframe window." A real Browser app
agent would be to web work what Writer is to living documents.

### 11. Tracing is also a plausible app-agent class

Tracing may also become an app agent if it:

- accepts prompts,
- drives dashboard state,
- issues complex queries,
- orchestrates retrieval or summarization primitives,
- presents a durable domain view rather than only passive logs.

That is enough to justify the category without deciding its full shape today.

## Non-Goals

- Designing every future app in this ADR
- Finalizing the Browser API
- Finalizing all app-to-app routing permutations
- Reclassifying every current component immediately
- Converting simple media viewers into app agents by default
- Choosing Go over Rust or Rust over Go

## Consequences

### Positive

- Writer gets a clear canonical role instead of being a moving target.
- The living-document model becomes a product foundation, not just a UI idea.
- Future app agents have a clear template.
- Cross-agent orchestration can grow without collapsing into a free-form mesh.
- The architecture stays meaningful in both Rust and Go.

### Negative

- We have to be stricter about which surfaces deserve app-agent status.
- Cross-agent calls require more contract work up front.
- Some currently fuzzy boundaries will have to be made explicit before new app
  work can proceed cleanly.

## Implementation Direction

### Phase 1: Finish Writer as the reference app agent

- harden the Writer contract,
- separate canon, marginalia, artifacts, and proposals,
- fix version and run-state invariants,
- keep `.writer_revisions` as migration-only compatibility state until removed.

### Phase 2: Define the agent request contract

- app agent to worker request envelope,
- app agent to app agent request envelope,
- capability and lease model,
- cancellation and timeout semantics,
- event emission requirements.

### Phase 3: Keep worker primitives reusable

- keep Terminal and Researcher reusable across app agents,
- keep Memory optional and query-oriented,
- avoid app-specific worker forks unless the domain genuinely requires them.

### Phase 4: Prototype the next app agent narrowly

- prefer Browser as the next app-agent spike,
- only then expand to Tracing or other domain-specific orchestration surfaces.

## Writer External API (Codesign Constraint)

The Writer API has been designed from the inside out: actor contracts, signal
flows, internal state management. The missing pressure is external consumers.

Two upcoming consumers force the API shape:

1. **Voice layer** (Gemini Live hackathon) — needs: what is in this document
   now? What changed? Do this thing and tell me when it is done.
2. **Publishing** — needs: what is the canonical version? Is it ready to be
   seen? What is the diff between draft and published?

Both are read-heavy external consumers of Writer state. They do not care about
actor internals. They need a stable surface they can poll or subscribe to
without understanding supervision trees, actor message types, or ractor
internals.

### Spatial vs temporal

Document state and document mutations are fundamentally different kinds of
data. Conflating them is the root of several current design bugs.

**Document state is spatial.** It is a shared, readable snapshot. All consumers
— frontend, voice client, publishing system, agent workers — should be able to
read the current document without sending a message and waiting for a reply.
This is a concurrent data structure with read-write mutex semantics (or the
equivalent in whatever runtime hosts it). Reads do not require coordination.
Reads do not block writes. The document is always available.

**Mutations and signals are temporal.** They are channeled, ordered, causal.
User edits (diffs) are legitimate temporal events: the user has write authority
and their changes flow through the mutation channel. Worker signals (findings,
results, questions) are also temporal, but they are NOT diffs. Workers do not
have write authority over the document. They contribute information that Writer
may choose to incorporate.

**The bug this model prevents:** workers sending diffs is workers using the
temporal interface to do spatial work (mutating the document) that is not
theirs. The fix is not changing the message format or adding validation on the
diff payload. The fix is making the write path physically impossible for
workers. Workers can send signals. Only Writer (and the user) can write to the
document. If the API does not enforce this at the type level, it will be
violated.

### One API for all consumers

The API should be the same for all consumers: Dioxus frontend, voice client,
publishing system, agent workers. If the frontend consumes a different API than
external clients, the contract is not real — it is an internal implementation
detail masquerading as a boundary.

This means:

- Document read (current state, version, metadata) is one endpoint or one
  subscription. Same shape for everyone.
- Document mutation (user edits, Writer revisions) flows through one write
  channel with identity and authority attached.
- Worker signals (findings, results, questions) flow through one signal
  channel that is explicitly not a write channel.
- Change notifications (what changed, when, by whom) are one subscription.
  Same shape for everyone.

If a voice client cannot get what it needs from the same API the frontend uses,
the API is wrong. If a publishing system needs a separate "export" endpoint
because the read API does not expose canonical state cleanly, the API is wrong.

### Implications for implementation phases

This constraint applies retroactively to Phase 1 (harden Writer contract) and
Phase 2 (define agent request contract). The external API shape should inform
the internal contract, not the other way around. Building the internal contract
first and then wrapping it for external consumption will produce a leaky
abstraction that drifts.

## Acceptance Signals

This ADR is succeeding if:

1. Writer is discussed and implemented as an app agent, not just as a file
   editor.
2. The living document remains the canonical human-facing record of a run.
3. Worker outputs support the document instead of replacing it with transcript
   UI.
4. New app proposals are evaluated against explicit app-agent criteria.
5. Cross-agent orchestration uses typed contracts rather than ad hoc mesh
   messaging.
6. The same architecture can be explained coherently in both `ractor` and Go
   terms.

## Related Documents

- `docs/practice/guides/writer-api-contract.md`
- `docs/archive/handoffs/2026-Q1/2026-02-14-living-document-human-interface-pillar.md`
- `docs/archive/handoffs/2026-Q1/2026-02-26-writer-living-document-system-review.md`
- `docs/state/reports/go-refactor-feasibility-2026-03-09.md`
