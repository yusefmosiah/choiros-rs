# Writer Control-Flow Checklist (Messaging-First, Trace-Only Events)

Date: 2026-02-14  
Status: Completed  
Owner: Current session

## Narrative Summary (1-minute read)

The regression was caused by a control-path gap: worker completion succeeded, but no writer patch message was guaranteed, so the run document stayed at bootstrap stub.

Direction is now explicit:
1. Control flow is actor messaging (`Worker -> WriterActor`), not EventStore.
2. EventStore keeps full telemetry for trace/audit/UI fanout.
3. Writer applies an immediate visible update when a message arrives, then runs its own LLM turn and applies a second update.

## What Changed

- Added a concrete execution checklist and linked each item to runtime behavior.
- Implemented Writer inbox queue semantics with dedupe and event-driven processing.
- Wired conductor worker completion/failure to enqueue typed messages into Writer.
- Added writer role to model policy resolution and moved conductor/writer defaults to `KimiK25`.
- Improved writer UI state handling when `writer.run.started` is missed, and surfaced latest writer run message in toolbar.

## What To Do Next

1. Run manual validation against a fresh run from prompt submit through completion.
2. Confirm the document now advances beyond bootstrap for worker summary runs.
3. Confirm the new writer role is visible/controllable in settings and policy files.
4. Add targeted integration tests for `EnqueueInbound -> writer.run.patch` behavior.

## Checklist

- [x] Define and enforce the rule: EventStore is trace-only, actor messages are control authority.
- [x] Add typed writer inbox message contract (`message_id`, `run_id`, `source`, `kind`, `content`).
- [x] Add writer message dedupe (`message_id`) to avoid duplicate patch writes on retries.
- [x] On inbound message, apply immediate proposal append so UI updates immediately.
- [x] Queue inbound message in WriterActor mailbox state.
- [x] Trigger bounded, event-driven writer LLM synthesis turn from inbox queue.
- [x] Apply second writer update from LLM synthesis output.
- [x] Keep writer/control activity mirrored to EventStore (`writer.actor.*`, `writer.run.*`, `llm.call.*`).
- [x] Wire conductor worker completion/failure to `WriterMsg::EnqueueInbound`.
- [x] Add first-class `writer` model role to model policy (`writer_default_model`, `writer_allowed_models`).
- [x] Set cost-saving defaults to Kimi for conductor and writer (`KimiK25`).
- [x] Update settings surface text to reflect writer role and Kimi defaults.
- [x] Improve UI run-state upsert so progress/status can render even if `writer.run.started` was missed.
- [x] Show latest live writer run message in Writer UI.

## Validation Targets

1. Prompt submit opens writer and shows bootstrap.
2. Worker completion emits `WriterMsg::EnqueueInbound` and document gains proposal text (revision increments).
3. Writer LLM synthesis emits a second proposal append (later revision increment).
4. Trace still shows `writer.run.*` and `llm.call.*` for the run.
5. No control-path step depends on replaying EventStore events.
