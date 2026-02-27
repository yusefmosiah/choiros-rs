# Agent Contract Hard Cutover (Tool-Calls Only)

## Narrative Summary (1-minute read)

Choir is cutting over to a strict agent contract: model output authority is typed tool calls only.
Unstructured top-level narrative fields are deprecated for control/output semantics. Writer is the
single owner of canonical living-document content and receives typed evidence/messages from
Researcher/Terminal. This is a hard cutover (non-backward-compatible) aimed at fixing three
failure classes: parse drift, status/meta leaks into canon, and weak source provenance.

The acceptance target is not only research UX. It is coding-agent UX: the living document must act
as the persistent feature checklist/plan that stays coherent while implementation work iterates,
including plan revisions mid-run without losing track of overall objective progress.

## What Changed

- Contract authority:
  - `AgentDecision` authority is `tool_calls`.
  - Completion is signaled by `finished` tool call only.
  - Top-level freeform decision text is not an authority channel.
- Writer authority:
  - Canonical document mutations are Writer-only (plus explicit user edits).
  - Researcher/Terminal cannot write canon directly.
  - Worker updates are typed `message_writer` envelopes with explicit source metadata.
- Source provenance:
  - Source objects must carry provider provenance (`tavily|brave|exa|...`) when discovered via
    `web_search`.
  - Citations must reference source IDs; no source parsing from markdown body.
- Identity framing:
  - System prompts should use role ownership framing:
    - "You are a member of the Choir Harmonic Intelligence Platform."
    - "Your role is <role>; you own <responsibility>."
  - Avoid generic "assistant" framing for multi-agent orchestration roles.

## Hard Cutover Rules (Non-Backward-Compatible)

1. No fallback to legacy decision message routing.
2. No markdown/body parsing for sources/citations.
3. No direct worker-to-canon mutations.
4. No hidden deterministic backup paths that mask contract violations.
5. Fail loudly with trace events when contract is violated.

## Turn-Sync Invariant (New, Mandatory)

For multi-agent runs, document context must refresh automatically on every agent turn.

1. Per-turn canonical snapshot:
  - Every delegated Researcher/Terminal turn receives the latest canonical writer document context.
  - Minimum required fields:
    - `run_id`
    - `canonical_version_id`
    - `canonical_document_markdown` (or bounded window + digest/hash)
2. Version-linked worker outputs:
  - Every worker `message_writer` update must include the base canonical version it used.
  - Writer can detect stale updates and reconcile explicitly (merge/rebase) when valid.
  - Staleness is expected in concurrent multi-agent runs; bounded staleness is allowed.
3. No hidden stale context:
  - Path-only hints are insufficient as execution context.
  - Worker prompts/system context must include actual document snapshot content (or canonical window policy).
4. Writer remains authority:
  - Worker updates are proposals/evidence tied to a canonical version.
  - Writer performs the canon mutation decision.
  - Writer may merge valid insights from stale proposals without requiring hard rejection.

## Canonical Typed Envelopes

- `message_writer` mode `proposal_append|canon_append|completion`:
  - `content: string`
  - `mode_arg.sources[]` with:
    - `id, kind, provider, url/path, title, publisher, published_at, line_start, line_end`
  - `mode_arg.citations[]` with:
    - `source_id, anchor`
- `finished`:
  - control signal only (`summary` allowed for compact machine/human audit text)
  - not a substitute for typed writer mutation paths

## Conformance Gates

1. Harness contract:
  - Reject decisions without valid `tool_calls`.
  - Do not treat top-level freeform decision text as canonical output authority.
2. Writer contract:
  - Status/progress/meta text must not replace canonical content.
  - Canonical content changes only via Writer revision path.
3. Source contract:
  - Source provider provenance survives transport and persistence.
  - Per-version source sets are persisted and retrieved with document versions.
4. Coding-agent UX contract:
  - A checklist-style plan can be revised mid-run while preserving whole-run coherence.
  - Delegated coding tasks update document state incrementally without derailing plan continuity.
5. Turn-sync contract:
  - Delegated agent turn payloads include canonical version + snapshot linkage.
  - Worker-to-writer updates carry base version provenance.
  - Stale updates are detectable, surfaced in telemetry, and handled via safe merge/rebase policy.

## Test Plan (Conformance-First)

- Add/extend tests to assert:
  - Harness ignores unstructured decision text for completion authority.
  - Writer does not accept status-only synthesis as canonical revision.
  - `message_writer` metadata includes provider and citation linkage.
  - Version navigation shows correct per-version source sets.
  - Researcher/Terminal turn context contains latest canonical document snapshot/version each turn.
  - Worker updates include base-version provenance and trigger stale-write handling when outdated.
  - Outdated proposals are not blindly rejected; valid stale insights are mergeable by Writer.
  - Coding run scenario:
    - initial 7-step checklist
    - mid-run plan change at step 5
    - final document remains coherent and correctly checked off.

## What To Do Next

1. Patch harness to complete from `finished` call semantics only.
2. Remove remaining prompt/context instructions that require top-level decision `message`.
3. Add `provider` to writer source metadata transport and storage surfaces.
4. Add conformance tests for harness output authority + writer non-meta-canon behavior.
5. Add coding-agent scenario test for checklist revision continuity.
