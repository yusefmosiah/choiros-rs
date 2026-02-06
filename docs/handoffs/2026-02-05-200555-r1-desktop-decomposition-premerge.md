# Handoff: Pre-Merge R1 Desktop Decomposition (sandbox-ui)

## Session Metadata
- Created: 2026-02-05 20:05:55 EST
- Project: /Users/wiz/.codex/worktrees/63c7/choiros-rs
- Branch: detached `HEAD` in this worktree
- Scope owner: R1 only (`desktop.rs` decomposition, no feature additions)

## Handoff Chain
- Continues from design spec: `/Users/wiz/choiros-rs/docs/design/2026-02-05-r1-dioxus-architecture-decomposition.md`
- Supersedes: None

## Current State Summary
- `sandbox-ui/src/desktop.rs` is reduced to a thin public entrypoint and keeps `Desktop` stable for `main.rs` imports.
- Desktop logic is decomposed into modular files under `sandbox-ui/src/desktop/` per R1 plan (shell, actions, effects, ws, theme, state, apps, and components).
- Chat/terminal behavior paths were preserved (prompt submit still focuses existing chat or opens new chat, then sends message).
- Backend-canonical state model remains intact (desktop state still sourced via API + websocket projection; reducers own mutation helpers).
- Work is currently uncommitted in this worktree.

## Architecture Overview
- `Desktop` remains the public entrypoint in `sandbox-ui/src/desktop.rs`.
- `DesktopShell` in `sandbox-ui/src/desktop/shell.rs` now orchestrates signals/effects and composes presentational components.
- Pure-ish helpers and transport/state projection are split into `apps.rs`, `theme.rs`, `ws.rs`, and `state.rs`.
- Async intent handlers and bootstrap side effects are split into `actions.rs` and `effects.rs`.

## Critical Files
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/shell.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/actions.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/effects.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/state.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/ws.rs`

## Files Added
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/actions.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/apps.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/components/mod.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/components/desktop_icons.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/components/prompt_bar.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/components/status_views.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/components/workspace_canvas.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/effects.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/shell.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/state.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/theme.rs`
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop/ws.rs`

## Files Modified
- `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/desktop.rs`

## Key Behavioral Equivalence Notes
- `Desktop` public entry signature unchanged.
- Initial effect ordering retained in `DesktopShell`:
  1. viewport bootstrap
  2. theme bootstrap (cache first, then backend)
  3. initial desktop fetch
  4. websocket bootstrap
  5. app registration (best-effort guard)
- Prompt submit logic retained:
  - focus existing chat window + send message, OR
  - open new chat window + local state push + send message.
- Window open/close/focus/move/resize action flows preserved through extracted `actions.rs`.

## Validation Performed (Exact Commands)
1. `cargo fmt --all`
- Result: pass.

2. `cargo check -p sandbox-ui`
- Result: pass.

3. `cargo test -p sandbox --test desktop_api_test`
- Result: pass (`17 passed, 0 failed`).

4. `cargo test -p sandbox-ui`
- Result: pass (0 tests in crate targets; lib/bin/doc test harness all pass).

5. `cargo clippy -p sandbox-ui -- -D warnings`
- Result: fails due pre-existing issues outside this lane in:
  - `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/components.rs`
  - `/Users/wiz/.codex/worktrees/63c7/choiros-rs/sandbox-ui/src/terminal.rs`
- These are unrelated to the R1 decomposition files above.

## Merge Agent Instructions
1. Inspect current delta:
- `git status --short`
- `git diff --name-status`

2. Keep merge scope limited to R1 files only:
- `sandbox-ui/src/desktop.rs`
- `sandbox-ui/src/desktop/**`
- (and this handoff file if desired)

3. Commit suggestion:
- Message: `refactor(sandbox-ui): decompose desktop.rs into R1 desktop modules`

4. Post-merge verification minimum:
- `cargo fmt --all`
- `cargo check -p sandbox-ui`
- `cargo test -p sandbox --test desktop_api_test`

## Immediate Next Steps
1. Stage only intended R1 files and commit in this worktree.
2. Merge/cherry-pick into the integration branch.
3. Run the minimum verification suite above on the merge target branch.
4. If integration branch enforces strict clippy, either:
- temporarily scope clippy to touched files/crates for this merge gate, or
- land a separate cleanup for existing `sandbox-ui/src/components.rs` and `sandbox-ui/src/terminal.rs` lint debt.

## Important Context
- This lane intentionally avoided new product behavior and kept all existing API payload contracts untouched.
- `desktop_window.rs` was intentionally not refactored in this lane; window drag/resize stubs remain as before.
- Some compile/lint turbulence during extraction was resolved in-module; final `cargo check -p sandbox-ui` is clean.

## Known Risks / Follow-ups
- `clippy -p sandbox-ui -D warnings` is not green due unrelated pre-existing lints in untouched files.
- No new wasm/component tests were added in this lane; confidence comes from compile + integration regression checks.
- If downstream lanes modify `sandbox-ui/src/desktop_window.rs`, retest prompt/chat + terminal window interactions after merge.

## Assumptions
- Current user identity remains hardcoded (`user-1`) as existing behavior.
- No concurrent merge lane is expected to rename or relocate the new `sandbox-ui/src/desktop/` module tree.

## Decisions Made
1. Keep `Desktop` export path and signature stable to avoid main/lib churn.
2. Keep presentational components API-driven from shell state; no direct API calls inside component modules.
3. Keep websocket parsing and event projection split (`ws.rs` -> `state.rs`) to preserve backend-canonical state updates.
