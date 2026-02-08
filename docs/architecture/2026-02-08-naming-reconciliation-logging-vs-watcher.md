# Naming Reconciliation: Logging vs Watcher

Date: 2026-02-08  
Status: Adopted terminology v1

## Narrative Summary (1-minute read)

We were overloading names. This caused architectural confusion between capture, detection, and narration.
This document sets a strict naming split:

- **Logging** = capture/store/transport of raw events.
- **Watcher** = deterministic detection/alerting from logs.
- **Summarizer** = derived, human-legible narratives over event batches.

This split keeps raw observability reliable while allowing clean UI summaries.

## What Changed

- Logs view now includes `worker.task`, `watcher.alert`, `model.*`, and `chat.*` events.
- Logs view renders a summary-first line for each event, with raw JSON behind collapsible details.
- Model routing metadata is now persisted in worker lifecycle events (`model_requested`, `model_used`).

## What To Do Next

- Add `SummarizerActor` to emit `log.summary.*` derived events.
- Keep raw `EventStore` rows canonical and immutable.
- Default UI to summaries; keep raw details on-demand.

## Naming Rules (v1)

- `*Actor`: runtime process boundary.
- `*Supervisor`: lifecycle/control hierarchy.
- `Logging`: event capture + persistence + fanout.
- `Watcher`: rule engine over events.
- `Summarizer`: narrative compression over event windows.

Avoid:
- Using `watcher` for generic logging transport.
- Using `logging` for alert policy logic.
- Using `agent/tool call` labels in user-facing UI where `action/operation/step` is clearer.
