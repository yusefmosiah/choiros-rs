# ChoirOS Narrative Index (Read This First)

Date: 2026-02-08
Purpose: Human-readable map of the architecture docs, in plain language.

## 60-Second Story

ChoirOS is shifting from parallel feature work to a linear, testable roadmap.
The current top deliverable is the `Directives` app: a first-class planning/control view.
Core architecture rule: direct actor-to-actor calls stay fast; deterministic policy checks enforce boundaries; policy actors are for high-risk escalation only.

## What We Are Building Right Now

1. Directives as the primary operator surface (app/window, not always-on).
2. Capability boundaries:
   - Chat can call `bash` interface but shell execution is delegated to TerminalActor.
   - PromptBar orchestrates actors and writes memos; it does not call tools.
3. Deterministic, reproducible operation:
   - model/config decisions are logged
   - events are the system of record

## Read Order (High-Level to Deep Dive)

1. `/Users/wiz/choiros-rs/docs/architecture/roadmap-dependency-tree.md`
   - Authoritative linear checklist and phase gates.
2. `/Users/wiz/choiros-rs/docs/architecture/directives-execution-checklist.md`
   - Product-level execution checklist and boundaries for Directives + policy pattern.
3. `/Users/wiz/choiros-rs/roadmap_progress.md`
   - What has already landed and what is next.
4. `/Users/wiz/choiros-rs/docs/architecture/worker-signal-contract.md`
   - Control-plane vs observability contract, typed turn reports, anti-spam rules.
5. `/Users/wiz/choiros-rs/docs/architecture/model-provider-agnostic-runbook.md`
   - Model/provider matrix and validation plan.
6. `/Users/wiz/choiros-rs/docs/architecture/pdf-app-implementation-guide.md`
   - Deferred guide-only milestone (no build yet).
7. `/Users/wiz/choiros-rs/docs/architecture/roadmap-critical-analysis.md`
   - Historical gap analysis and risks (use as reference, not current ordering authority).

## Current Decisions (Explicit)

- Roadmap is linear, not parallel by default.
- One active milestone at a time; pass gate before moving on.
- Directives app is prioritized over PDF app implementation.
- Policy actor is not first-line routing.
- Agentic policy is deferred to hardening/post-deployment.

## One-Line Summary Per Core Doc

- `roadmap-dependency-tree.md`: "What order we execute in, and why."
- `directives-execution-checklist.md`: "What must be true before we call this architecture real."
- `roadmap_progress.md`: "What changed and what we tackle next."
- `model-provider-agnostic-runbook.md`: "How to prove model routing and provider support."
- `worker-signal-contract.md`: "How workers decide what to signal, without spamming."
- `pdf-app-implementation-guide.md`: "What PDF should become later, without building it now."
- `roadmap-critical-analysis.md`: "Where the earlier plan overestimated readiness."

## Doc Readability Rule (Human-First)

For major architecture docs, include a top section:
- `Narrative Summary (1-minute read)`
- `What Changed`
- `What To Do Next`

If a doc is long, it is not done until this summary exists.
