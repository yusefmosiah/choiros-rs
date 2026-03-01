# Living-Document Human Interface Pillar

Date: 2026-02-14  
Status: Key design pillar (authoritative)  
Scope: Human AI interaction model, conductor handoff path, product naming/domain direction

## Narrative Summary (1-minute read)

ChoirOS is living-document-first for human AI interaction.
There is no standalone chat app in the active product direction.

Humans interact through a durable living document surface.
That surface sends actor messages to Conductor for orchestration, and Conductor delegates to workers/app agents.

This aligns UX with system architecture:
1. durable artifact first,
2. orchestration second,
3. replayable event trace always.

Domain direction follows the same principle:
`choir.chat` implied ephemeral interaction, while `choir-ip.com` emphasizes enduring value and durable outputs.

## What Changed

1. Declared living-document UX as the primary human interface.
2. Removed chat-app framing from active architecture policy.
3. Aligned conductor orchestration language around living-document handoff.
4. Recorded domain rationale: durable IP over ephemeral chat identity.

## What To Do Next

1. Audit active docs to remove chat-app assumptions from current-state sections.
2. Keep historical chat references only in clearly archived sections.
3. Ensure living-document UX events map cleanly to conductor wake messages.
4. Validate that run narratives and artifacts remain first-class in UX and APIs.

---

## Core Pillar Statement

Human interaction is document-native and durable.
Conductor remains orchestration authority behind that interface.

## Interface Contract (Human -> Conductor)

1. Human input enters through living-document surfaces.
2. Input is packaged as typed actor messages with natural-language objectives.
3. Conductor decides delegation and returns updates as structured events.
4. UI renders semantic run progress and persistent artifacts, not transient chat-only turns.

## Domain/Identity Rationale

Why `choir-ip.com` over `choir.chat`:
1. `chat` describes a modality, not the system's durable value.
2. ChoirOS is producing enduring artifacts, decisions, traces, and revisions.
3. "IP" better matches long-lived outputs and operating-system-level composition.

## Acceptance Signals

1. Primary UX is a living document, not a chat transcript app.
2. Conductor orchestration is initiated from living-document actor messages.
3. Docs and roadmap describe chat only as historical context where needed.
4. Product language consistently emphasizes durable artifacts and run narratives.
