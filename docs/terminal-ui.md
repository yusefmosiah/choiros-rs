# Terminal UI Integration (Dioxus + xterm.js)

## Goal
Provide a fully interactive terminal window inside the Dioxus desktop UI, backed by the existing
`/ws/terminal/{terminal_id}` WebSocket endpoint and rendered with xterm.js.

## Architecture Overview
- Dioxus owns layout, window chrome, and lifecycle.
- xterm.js owns rendering, cursor state, ANSI parsing, and key handling.
- A small JS bridge creates and manages the xterm instance and forwards input/output.
- Rust/WASM owns the WebSocket connection and JSON protocol.

```mermaid
flowchart LR
  A[TerminalView (Dioxus)] -->|mount| B[JS Bridge]
  B -->|create/open| C[xterm.js Terminal]
  A -->|ws connect| D[WS /ws/terminal/{id}]
  C -->|onData| A
  A -->|input JSON| D
  D -->|output JSON| A
  A -->|write| C
```

## Message Protocol
The backend expects JSON, tagged by `type`.
- `input`: `{ "type": "input", "data": "..." }`
- `resize`: `{ "type": "resize", "rows": 24, "cols": 80 }`
- `output`: `{ "type": "output", "data": "..." }`
- `info`: `{ "type": "info", "terminal_id": "...", "is_running": true }`
- `error`: `{ "type": "error", "message": "..." }`

Notes:
- Enter should send `\r` (carriage return). xterm emits that by default via `onData`.
- Output data can include ANSI escape sequences. xterm.js handles rendering.

## WebSocket URL
`ws://{host}/ws/terminal/{terminal_id}?user_id=user-1`

`terminal_id` can be the `WindowState.id` so each window is a distinct session.

## Frontend Component Plan
### `TerminalView` (Rust, Dioxus)
Responsibilities:
- Allocate a container `div` and pass its id to the JS bridge.
- Create xterm via JS on mount and dispose on unmount.
- Open WebSocket and translate messages to `xterm.write()`.
- Forward xterm `onData` to the WebSocket as `input` messages.
- Recalculate rows/cols on size changes and send `resize`.

Suggested lifecycle pattern:
- `use_node_ref` for the container element.
- `use_effect` to create and connect once on mount.
- `use_drop` to close WebSocket and dispose xterm on unmount.

### `terminal.rs` (Rust)
Public API:
- `TerminalView { terminal_id: String }`

Internal helpers:
- `connect_terminal_ws(terminal_id, on_output)`
- `send_ws_msg(ws, msg)`

## JS Bridge Plan
Create `sandbox-ui/assets/terminal.js` that exports:
- `createTerminal(container, options) -> handle`
- `writeTerminal(handle, data)`
- `resizeTerminal(handle, rows, cols)`
- `onTerminalData(handle, callback)`
- `disposeTerminal(handle)`

The bridge holds a map of terminal handles to xterm instances.
Use the xterm `FitAddon` to size to container.

## Assets
- `sandbox-ui/public/terminal.js`
- `sandbox-ui/public/xterm.js` (xterm.js 5.3.0)
- `sandbox-ui/public/xterm-addon-fit.js` (fit addon 0.8.0)
- `sandbox-ui/public/xterm.css`

`sandbox-ui/dioxus.toml` sets `asset_dir = "public"` so `dx serve` serves these at `/`.

Include CSS in Dioxus via a `link` tag in the top-level app or in `TerminalView`:
`<link rel="stylesheet" href="/assets/xterm.css">`

## Resizing Strategy
- Use `FitAddon` to compute the closest rows/cols based on container size.
- On window resize, call `fitAddon.fit()`, then read `term.rows` / `term.cols` and send `resize`.
- Throttle resize events (50-100ms) to avoid flooding the server.

## Error Handling
- If WS fails, show a simple overlay inside the terminal window.
- On reconnect, create a new session with the same `terminal_id`.
- Optional backend improvement: on connect, call `TerminalMsg::GetOutput` and send
  buffered output before streaming live output.

## Implementation Status (2026-02-05)
- JS bridge added in `sandbox-ui/assets/terminal.js` using xterm.js + fit addon.
- `TerminalView` implemented in `sandbox-ui/src/terminal.rs`.
- `TerminalView` wired into `sandbox-ui/src/desktop.rs` for app id `terminal`.
- `xterm.css` linked in the desktop root.

Open work:
- Consider improving reconnect UX (surface countdown, manual retry button).

## Phase Plan
1. Add JS bridge and xterm assets. (done)
2. Implement `TerminalView` and wire it into `desktop.rs`. (done)
3. Add WS glue and resize handling. (done)
4. Add reconnect + simple status UI. (done)
5. Optional backend replay on connect. (done)

## E2E Smoke Script
Run from repo root after `just dev-sandbox` + `just dev-ui`:

```bash
scripts/e2e_terminal.sh http://localhost:3000 tests/screenshots/terminal-e2e.png
```
