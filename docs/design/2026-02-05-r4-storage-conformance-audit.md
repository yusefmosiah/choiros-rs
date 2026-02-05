# R4 - Storage Conformance Audit

**Date:** 2026-02-05
**Status:** In progress

## Scope

Audit planned and current UI features against storage reconciliation policy and define required remediations.

## Inputs

- `docs/design/2026-02-05-ui-storage-reconciliation.md`
- `docs/design/2026-02-05-ui-implementation-backlog.md`
- `sandbox-ui/src/api.rs`
- `sandbox-ui/src/components.rs`
- `sandbox-ui/src/desktop.rs`
- `sandbox/src/api/user.rs`

## Policy Gates

1. Backend/EventStore canonical.
2. Browser storage non-authoritative.
3. Backend wins on conflict.
4. UI has no local source-of-truth for domain state.

## Conformance Matrix (Initial)

| Feature | Status | Notes |
|---|---|---|
| Theme preference persistence | Pass (provisional) | Backend + local cache path exists; verify conflict path explicitly. |
| Chat history hydration HTTP + WS | Partial | Requires explicit payload-shape parity tests. |
| Window state persistence | Pass (backend) / Partial (ui interaction) | Event persistence exists; frontend interaction fidelity still incomplete. |
| Viewer persistence design | Pending | Must be defined backend-first before implementation. |

## Required Remediation Items (Draft)

1. Add explicit tests for backend-over-cache conflict resolution in theme bootstrap.
2. Add integration tests for chat tool event parity across HTTP history + WS stream.
3. Define reconciliation behavior for future viewer caches before viewer code lands.

## Acceptance Checklist

- [ ] Matrix expanded to all planned features.
- [ ] Every partial/fail has concrete remediation task.
- [ ] Test requirements linked to exact endpoints/components.
