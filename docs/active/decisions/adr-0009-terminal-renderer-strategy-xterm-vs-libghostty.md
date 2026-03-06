# ADR-0009: Terminal Renderer Strategy (xterm.js vs Ghostty/libghostty)

Date: 2026-03-01
Status: Proposed
Owner: Desktop / Runtime

## Narrative Summary (1-minute read)

ChoirOS terminal startup is currently reported at approximately 5-10 seconds in local usage.
Current code shows startup can spend up to about 11 seconds in frontend wait loops before a usable
session appears (`ensure_terminal_scripts` + container wait + reconnect backoff).

Replacing `xterm.js` with native `libghostty` is not a direct drop-in for the current
browser/hypervisor path: current UI is web-rendered Dioxus and depends on a browser terminal widget.
`libghostty` is also explicitly not yet stable as a standalone API.

Decision direction:

1. Keep the current browser terminal contract for now.
2. Fix startup latency first in the existing `xterm.js` path.
3. Run a time-boxed spike for a web-compatible Ghostty path (`ghostty-web`, WASM) if needed.
4. Revisit native `libghostty` embedding only if we move terminal surfaces to native desktop UI
   (non-web) or split runtime modes by platform.

## What Changed

1. Documented current ChoirOS terminal startup path and delay budget from repository code.
2. Compared three options:
   - Optimize current `xterm.js` integration.
   - Replace browser renderer with `ghostty-web` (WASM).
   - Replace with native `libghostty`.
3. Defined a phased execution plan with measurable acceptance criteria.

## What To Do Next

1. Add startup timing instrumentation in frontend and backend.
2. Ship low-risk xterm startup fixes first (preload and remove polling bottlenecks).
3. Run a short `ghostty-web` spike behind a feature flag.
4. Decide on migration only after measured p50/p95 startup improvements and compatibility results.

## Context

### Current Architecture (ChoirOS)

Current web terminal flow:

1. `TerminalView` in Dioxus Web mounts terminal UI.
2. Frontend dynamically injects `/xterm.js`, `/xterm-addon-fit.js`, and `/terminal.js`.
3. Frontend waits for globals (`Terminal`, `FitAddon`, `createTerminal`) by polling.
4. WebSocket connects to `/ws/terminal/{terminal_id}`.
5. Backend ensures terminal actor exists, starts PTY, and streams output.

Relevant code:

- `dioxus-desktop/src/terminal.rs`:
  - init flow and script waits (`ensure_terminal_scripts`, `wait_for_js_global`)
  - container wait (`wait_for_terminal_container`)
  - reconnect backoff (`schedule_reconnect`)
- `dioxus-desktop/public/terminal.js`:
  - xterm creation and fit/write bridge
- `sandbox/src/api/terminal.rs`:
  - terminal websocket startup and actor wiring
- `sandbox/src/actors/terminal.rs`:
  - PTY spawn path (`spawn_pty`)

### Delay Budget Observed in Code

Without any backend slowness, current frontend flow can consume:

1. Up to 3.0s waiting for `Terminal` global.
2. Up to 3.0s waiting for `FitAddon` global.
3. Up to 3.0s waiting for `createTerminal` global.
4. Up to 2.0s waiting for container element.

Total worst-case before stable WS open attempt: about 11s.

This aligns with observed 5-10s perceived startup.

## Problem Statement

We need significantly faster terminal startup while preserving:

1. Browser-based runtime compatibility through hypervisor ingress (`localhost:9090` path).
2. Existing websocket/PTY behavior and typed observability contracts.
3. Cross-environment reliability (local vfkit, hypervisor, prod runtime routing).

## Decision Drivers

1. Startup latency (time to visible prompt and usable input).
2. Browser compatibility.
3. Integration complexity and risk.
4. API stability and long-term maintenance.
5. Test coverage and migration safety.

## Options Considered

### Option A: Keep xterm.js, fix startup path first

Scope:

1. Preload scripts once at shell bootstrap (or include script tags in index for terminal bundle).
2. Replace polling loops with `onload`/Promise-based script readiness.
3. Parallelize script fetch where safe.
4. Remove/reduce container polling where mount lifecycle guarantees exist.
5. Add server-side compression/cache headers for static terminal assets if missing.

Entailed work:

1. Frontend terminal loader refactor in `dioxus-desktop/src/terminal.rs`.
2. Optional static asset pipeline changes in sandbox static serving path.
3. Add perf telemetry markers:
   - `terminal.ui.mount`
   - `terminal.scripts.ready`
   - `terminal.ws.open`
   - `terminal.first.output`
4. Add Playwright assertion for startup SLA.

Pros:

1. Lowest risk and shortest path.
2. No protocol or rendering-contract rewrite.
3. Directly addresses largest measured delay budget.

Cons:

1. Renderer remains xterm.js.
2. Does not validate Ghostty-based path.

### Option B: Browser replacement via `ghostty-web` (WASM)

Scope:

1. Swap terminal bridge implementation to a `ghostty-web` adapter.
2. Keep same websocket protocol and backend actor/PTy path.
3. Maintain compatibility with existing UI hooks (`onData`, `fit`, resize, write).

Entailed work:

1. Add WASM init lifecycle (`init()` once early).
2. Update terminal bridge JS and Rust wasm_bindgen externs.
3. Validate compatibility with existing xterm-like API assumptions.
4. Add fallback to xterm.js behind feature flag.

Pros:

1. Preserves browser architecture.
2. Potentially better emulation behavior from Ghostty core in web context.

Cons:

1. Additional WASM initialization and payload.
2. Third-party integration risk and maturity risk.
3. Startup speed gain is uncertain until measured; cold-start could be slower.

### Option C: Native `libghostty` embedding (replace browser renderer)

Scope:

1. Build native terminal surface in desktop app using `libghostty` C API.
2. Route input/output through existing websocket or direct runtime channels.
3. Maintain parity with browser mode or split platform capabilities.

Entailed work:

1. Major architecture shift from web terminal view to native widget embedding.
2. Platform-specific UI integration per OS toolkit.
3. New abstraction for renderer parity between browser and native modes.
4. Extended test matrix and dual-mode support strategy.

Pros:

1. Long-term path to native Ghostty-powered rendering if desktop-native UI becomes primary.

Cons:

1. Not a drop-in for current web deployment path.
2. Highest implementation risk and cost.
3. `libghostty` standalone API currently explicitly unstable.

## Decision

Adopt a phased strategy:

1. Phase 1: Optimize current xterm startup path and measure.
2. Phase 2: Optional `ghostty-web` spike under feature flag if Phase 1 misses targets.
3. Phase 3: Revisit native `libghostty` only with a broader native desktop surface decision.

Native `libghostty` replacement is not approved as the immediate path for the current web runtime.

## Could Ghostty/libghostty Improve Startup Times?

Short answer: possibly, but not by itself in current architecture.

1. Current 5-10s is likely dominated by frontend wait/poll logic and startup sequencing, not PTY spawn.
2. Replacing renderer alone does not remove those waits unless loader/init flow is redesigned.
3. A web Ghostty path (`ghostty-web`) may improve emulation characteristics, but startup latency remains
   uncertain until measured against xterm with equivalent loader fixes.
4. Native `libghostty` could improve some desktop render characteristics in a native app mode, but it does
   not directly solve current browser startup costs.

## Implementation Plan

### Phase 1 (Immediate, low risk)

1. Add timing probes around:
   - script injection start/end
   - terminal creation
   - websocket open
   - first output chunk
2. Replace polling-based script readiness with load-event readiness.
3. Preload terminal assets on desktop shell boot (not first terminal open).
4. Enforce startup SLO in Playwright:
   - target p50 <= 2s
   - target p95 <= 4s

### Phase 2 (Conditional spike)

1. Build `ghostty-web` adapter behind `CHOIR_TERMINAL_RENDERER=ghostty_web`.
2. Run A/B startup and compatibility suite:
   - interactive shell prompt
   - resize behavior
   - vim/tmux rendering
   - clipboard/input correctness
3. Promote only if faster or functionally superior without regression.

### Phase 3 (Strategic)

1. Evaluate native renderer track only if desktop-native UI mode is prioritized.
2. Define a dual-renderer abstraction (browser + native) before implementation.

## Test and Validation

1. Extend `tests/playwright/vfkit-terminal-proof.spec.ts` with explicit startup timing assertions.
2. Keep websocket integration tests in `sandbox/tests/terminal_ws_smoketest.rs` unchanged to protect transport behavior.
3. Add regression tests for resize/input/output under reconnect.
4. Capture artifacts (trace/video) for startup benchmarking across 9090 ingress path.

## Risks

1. Hidden compatibility regressions in terminal escape handling and keyboard input behavior.
2. Added bundle/init cost from WASM path.
3. Maintaining two renderers during migration period.
4. Over-attributing delay to renderer while startup sequencing remains primary bottleneck.

## Rollback Plan

1. Keep xterm.js as default renderer until feature-flag evaluation passes.
2. Runtime flag-controlled renderer selection allows immediate rollback without backend changes.

## External Source Notes

1. Ghostty docs: architecture, native scope, libghostty status:
   - https://ghostty.org/docs/about
2. Ghostty repo notes on `libghostty-vt` and current API stability:
   - https://github.com/ghostty-org/ghostty
3. libghostty roadmap from author:
   - https://mitchellh.com/writing/libghostty-is-coming
4. `ghostty-web` project (WASM Ghostty core with xterm-like API):
   - https://github.com/coder/ghostty-web
5. xterm.js official docs and addon model:
   - https://github.com/xtermjs/xterm.js
   - https://xtermjs.org/docs/guides/using-addons/

## Repo References

1. `dioxus-desktop/src/terminal.rs`
2. `dioxus-desktop/public/terminal.js`
3. `dioxus-desktop/public/xterm.js`
4. `sandbox/src/api/terminal.rs`
5. `sandbox/src/actors/terminal.rs`
