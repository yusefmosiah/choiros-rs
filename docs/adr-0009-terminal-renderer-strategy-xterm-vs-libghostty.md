# ADR-0009: Terminal Renderer Strategy (xterm.js vs Ghostty/libghostty)

Date: 2026-03-15
Kind: Decision
Status: Proposed
Priority: 5
Requires: []
Owner: Desktop / Runtime

## Narrative Summary (1-minute read)

ChoirOS terminal startup is currently perceived as slow in local use, roughly 5-10 seconds.
The current repository shows a clear frontend-side delay budget before a usable terminal can
appear: up to about 11 seconds on the happy path from script/global polling and container wait
loops alone. Reconnect backoff adds more delay only after failures.

Replacing `xterm.js` with native `libghostty` is not a direct drop-in for the current runtime.
ChoirOS renders its terminal in a browser-hosted Dioxus UI and talks to the backend over a
websocket PTY bridge. Native `libghostty` is designed for native app embedding, and Ghostty's own
docs still describe the standalone `libghostty` API as not yet stable.

Decision direction:

1. Keep the browser terminal contract for now.
2. Treat startup latency as a sequencing/instrumentation problem first, not a renderer swap
   problem.
3. Optimize the current `xterm.js` path before any renderer migration.
4. Run a small `ghostty-web` spike only if measured latency or terminal-behavior gaps remain.
5. Revisit native `libghostty` only if ChoirOS adopts a true native desktop terminal surface.

## What Changed

1. Revalidated the terminal startup path against current repo code on 2026-03-15.
2. Split known frontend delay budget from inferred backend costs.
3. Clarified that reconnect backoff is a failure-path delay, not part of the normal first-load
   budget.
4. Narrowed the immediate decision: do not pursue native `libghostty` as the current fix for slow
   startup.

## What To Do Next

1. Add timing instrumentation from terminal mount through first output.
2. Remove polling-heavy startup waits in the existing `xterm.js` path.
3. Add a Playwright startup SLA check to the existing terminal proof.
4. Only run a `ghostty-web` comparison after Phase 1 measurements are in.

## Context

### Current ChoirOS Terminal Path

The current terminal surface is browser-rendered, not native:

1. `dioxus-desktop/src/main.rs` launches the Dioxus app with WASM logging.
2. `TerminalView` in `dioxus-desktop/src/terminal.rs` creates the terminal UI.
3. The frontend injects `/xterm.js`, `/xterm-addon-fit.js`, and `/terminal.js`.
4. The frontend waits for the `Terminal`, `FitAddon`, and `createTerminal` globals.
5. The frontend opens `ws://.../ws/terminal/{terminal_id}`.
6. `sandbox/src/api/terminal.rs` ensures a `TerminalActor` exists and sends `Start`.
7. `sandbox/src/actors/terminal.rs` spawns the PTY and shell via `portable_pty`.

The current frontend renderer contract is therefore:

1. Browser DOM container
2. JS terminal widget
3. Websocket transport
4. Backend PTY actor

Any native `libghostty` path would change that contract for at least one platform.

### Evidence From Current Code

Frontend startup path:

1. `ensure_terminal_scripts` injects `xterm.js`, `xterm-addon-fit.js`, and `terminal.js`.
2. It then polls for `Terminal`, `FitAddon`, and `createTerminal` with
   `wait_for_js_global(name, 30, 100)`.
3. `wait_for_terminal_container(container_id, 40, 50)` polls for the terminal DOM node.
4. If the container or bridge init fails, `schedule_reconnect` starts exponential backoff.

Backend startup path:

1. `terminal_websocket` upgrades the socket and calls `ensure_started_terminal`.
2. `ensure_started_terminal` gets or creates the actor and sends `TerminalMsg::Start`.
3. `TerminalMsg::Start` calls `spawn_pty`.
4. `spawn_pty` opens a native PTY, spawns the requested shell, and sets up reader/writer tasks.

This means the repo already proves a substantial frontend-side delay budget. It does not yet prove
how much of the residual user-perceived delay comes from PTY spawn, sandbox/container readiness, or
first shell output because those stages are not instrumented with timing markers today.

### Delay Budget Visible in Code

Known happy-path frontend waits:

1. `Terminal` global wait: up to 3.0s
2. `FitAddon` global wait: up to 3.0s
3. `createTerminal` global wait: up to 3.0s
4. Terminal container wait: up to 2.0s

Total known frontend wait budget before a usable session can appear: about 11.0s.

Failure-path reconnect delay:

1. First reconnect attempt is about 0.8-1.2s
2. Later attempts grow exponentially
3. Delay caps around 6.4-9.6s per attempt
4. Maximum attempts: 6

Reconnect backoff is therefore important for recovery behavior, but it should not be counted as
baseline startup delay unless the initial attempt is already failing.

## Problem Statement

We need to reduce time-to-usable-terminal without breaking:

1. Browser-based terminal access through the current hypervisor ingress path
2. Existing websocket PTY transport and actor supervision model
3. Current observability and test strategy
4. Cross-environment behavior across local vfkit and deployed runtime paths

## Decision Drivers

1. Startup latency to first usable prompt
2. Browser compatibility
3. Implementation risk
4. API stability
5. Migration complexity and test burden

## Options Considered

### Option A: Keep `xterm.js`, fix the startup path first

Scope:

1. Replace polling-based script readiness with explicit load readiness or eager preload.
2. Stop paying the three sequential 3-second wait budgets on first terminal open.
3. Reduce or eliminate container polling when the mount lifecycle can provide the element
   directly.
4. Add timing markers for mount, scripts ready, websocket open, and first output.
5. Extend Playwright to assert startup SLOs.

Pros:

1. Directly targets the currently verified delay budget.
2. Preserves the current browser/websocket/PTY contract.
3. Lowest migration cost and lowest compatibility risk.
4. Gives hard measurements before making a renderer decision.

Cons:

1. Keeps `xterm.js` as the renderer.
2. Does not address any potential terminal emulation gaps that may motivate Ghostty separately.

### Option B: Replace the browser renderer with `ghostty-web`

Scope:

1. Swap the JS bridge to a `ghostty-web` adapter.
2. Keep the backend websocket and PTY contract intact.
3. Initialize Ghostty's WASM payload early and bridge its API into the current terminal window.

Pros:

1. Still compatible with the current browser-based architecture.
2. Potentially improves terminal behavior if Ghostty's emulation is materially better for ChoirOS
   use cases.
3. `ghostty-web` advertises an xterm-compatible API, which lowers adaptation risk relative to a
   full rewrite.

Cons:

1. Startup speed is uncertain until measured; WASM init may help or hurt cold start.
2. Adds third-party maturity risk; `ghostty-web` is explicitly under heavy development and does
   not promise compatibility guarantees yet.
3. Still requires frontend loader work, so it does not avoid the current sequencing problem by
   itself.

### Option C: Replace the renderer with native `libghostty`

Scope:

1. Build a native terminal surface for a native desktop app mode.
2. Define how native desktop mode and browser mode coexist, diverge, or are split by platform.
3. Rework the renderer abstraction around two UI surface models, not just one terminal library
   swap.

Pros:

1. Plausible long-term path if ChoirOS intentionally becomes a native desktop application with
   native terminal surfaces.
2. Aligns with Ghostty's intended native embedding model.

Cons:

1. Not a drop-in replacement for the current browser-rendered terminal.
2. Does not solve the current web runtime path used through hypervisor/browser access.
3. Introduces a dual-mode product surface unless browser support is explicitly dropped.
4. Depends on a standalone `libghostty` API that Ghostty docs still describe as not yet stable.

## Decision

Adopt Option A as the immediate path.

1. Keep `xterm.js` as the default terminal renderer.
2. Fix startup sequencing and add timing instrumentation first.
3. Defer any native `libghostty` work; it is not approved as the current latency fix.
4. Permit a bounded `ghostty-web` spike only after Phase 1 measurements if there is still an
   unresolved latency or compatibility case.

## Rationale

The strongest evidence currently in-repo points to frontend wait loops, not to renderer intrinsic
cost, as the main known source of terminal startup delay. A renderer migration that keeps the same
browser architecture would still need startup loader cleanup. A native `libghostty` migration would
require an architecture change beyond the scope of "make terminal startup faster."

This is therefore a sequencing decision before it is a renderer decision.

## Consequences

### Positive

1. Fastest path to a measurable startup improvement.
2. Preserves current web terminal compatibility and avoids a split-brain renderer story for now.
3. Produces benchmark data that can justify or kill future renderer work cleanly.

### Negative

1. ChoirOS remains on `xterm.js` in the near term.
2. If the real bottleneck turns out to be elsewhere, Phase 1 may not fully solve the problem.
3. A later `ghostty-web` spike still creates evaluation and regression work.

## Implementation Plan

### Phase 1: Instrument and optimize the current path

1. Add timings for:
   - terminal mount start
   - script readiness complete
   - terminal widget created
   - websocket open
   - first output received
2. Replace `wait_for_js_global(..., 30, 100)` loops with deterministic readiness.
3. Preload terminal assets earlier in app lifetime if that reduces first-open latency.
4. Remove unnecessary DOM polling if the component lifecycle can hand over the container directly.

### Phase 2: Add a startup regression check

1. Extend `tests/playwright/vfkit-terminal-proof.spec.ts`.
2. Record time from terminal open action to visible connected/usable terminal.
3. Gate on explicit targets after initial measurement.

Suggested initial targets:

1. p50 <= 2s
2. p95 <= 4s

These are targets, not current measured repo-backed results.

### Phase 3: Conditional `ghostty-web` spike

1. Add a feature-flagged `ghostty-web` adapter.
2. Compare:
   - cold start time
   - warm start time
   - resize correctness
   - prompt visibility
   - vim/tmux behavior
   - keyboard and clipboard behavior
3. Keep `xterm.js` as the rollback default during evaluation.

### Phase 4: Strategic native-only discussion

1. Reopen native `libghostty` only if ChoirOS commits to a native desktop terminal surface.
2. Make that decision in a separate ADR, because it changes UI architecture rather than only a
   library choice.

## Test and Validation

Current state:

1. `tests/playwright/vfkit-terminal-proof.spec.ts` verifies eventual terminal connectivity and
   basic command execution.
2. `sandbox/tests/terminal_ws_smoketest.rs` and related terminal tests protect the websocket/PTy
   transport behavior.
3. There is no current startup SLA assertion in the Playwright proof.

Validation needed:

1. Timing instrumentation in frontend and backend
2. Browser E2E startup assertions on `localhost` origins
3. Renderer comparison tests only after baseline `xterm.js` startup cleanup lands

## Risks

1. Over-focusing on the renderer when loader sequencing is the actual bottleneck
2. Underestimating backend or runtime readiness costs because they are not yet timed explicitly
3. Adopting `ghostty-web` too early while its compatibility surface is still moving
4. Treating native `libghostty` as a small swap when it is actually an architecture split

## Rollback Plan

1. Keep `xterm.js` as the default renderer until any alternative passes startup and compatibility
   gates.
2. If a `ghostty-web` spike regresses behavior, disable it with a runtime/build flag and keep the
   current websocket/PTY backend untouched.

## External Source Notes

These sources were checked on 2026-03-15:

1. Ghostty docs describe Ghostty as a native terminal emulator and state that `libghostty` is used
   by the macOS and Linux GUIs, but is not yet a stable standalone API:
   - https://ghostty.org/docs/about
2. `ghostty-web` describes itself as a web terminal built on Ghostty via WebAssembly, with an
   xterm.js-compatible API, but says it is under heavy development with no compatibility
   guarantees:
   - https://github.com/coder/ghostty-web
