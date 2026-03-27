---
name: rename-worker
description: Execute scoped repo-local cutover and rename work for the choiros-rs cagent→cogent mission.
---

# Rename Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the work procedure for implementation features in this mission.

## When to Use This Skill

Use this skill for features that:
- rename repo-local `cagent` surfaces to `cogent`
- cut over `.cagent/` state to `.cogent/`
- remove deprecated adapter-era code/config/script surfaces
- fix validation blockers required to make the cutover green
- update active operator docs that participate in the live cutover contract

## Required Skills

None.

## Work Procedure

1. Read the assigned feature, `mission.md`, `AGENTS.md`, and the relevant `.factory/library/*.md` files before editing anything.
2. Re-scan only the files in scope for the feature. Do not broaden scope to ATLAS removal, archive cleanup, or `~/cogent`.
3. If the feature changes Rust behavior, add or update tests first so the changed contract is exercised before implementation.
4. For state-root changes, use `git mv` for tracked paths and preserve tracked artifact content. `supervisor.json` is removed, not renamed.
5. For runtime/package/script changes, make the repo-local hard cutover explicit:
   - use `cogent`, not `cagent`
   - remove deprecated adapter surfaces (`codex`, `opencode`, `--codex-openai-bridge`, `.codex/`, `.opencode/`, `.gemini/`, `.pi/`) from active surfaces in scope
   - keep ATLAS removal out of scope
6. For docs features, only update active/current docs described by the feature. Preserve the recently committed flattened docs layout.
7. Run the smallest useful validation first, then broaden to the feature’s required checks:
   - Baseline-unblock features are special: if the feature description explicitly says it is clippy-only, test-only, or otherwise scoped to one validator, treat unrelated pending baseline failures as tracked elsewhere and do not let them block completing the assigned feature
   - Rust-only feature: targeted tests, then `cargo check --workspace --locked`
   - Nix feature: relevant `nix eval` / `nix build`, then `nix flake check --no-build --no-write-lock-file`
   - Docs/script feature: targeted grep or script help checks, plus any required regeneration step
   - Do not run expensive live-model tests unless the feature explicitly requires them; this mission disables those by default
8. Before finishing, run every verification step listed on the feature. If a validator is already red because of unrelated pre-existing work, call it out explicitly in the handoff.
9. Commit only the feature’s implementation changes. Do not push.

## Example Handoff

```json
{
  "salientSummary": "Cut over the repo state root from .cagent to .cogent, removed supervisor.json, and updated ignore rules so runtime churn stays clean. Verified the renamed state files and preserved tracked artifacts under .cogent.",
  "whatWasImplemented": "Renamed tracked repo state paths with git mv, removed the tracked supervisor.json loose end, updated .gitignore for .cogent sidecars/runtime directories, and adjusted the relevant docs/config paths so repo-local state now points at .cogent/cogent.db and .cogent/cogent-private.db.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {
        "command": "git ls-files .cogent",
        "exitCode": 0,
        "observation": "Tracked artifact paths moved under .cogent and supervisor.json is absent."
      },
      {
        "command": "cargo check --workspace --locked",
        "exitCode": 0,
        "observation": "Workspace still compiles after the state-path updates."
      },
      {
        "command": "rg -n '\\.cagent|supervisor\\.json' README.md CLAUDE.md AGENTS.md .gitignore",
        "exitCode": 1,
        "observation": "No remaining active-surface references in the files touched by this feature."
      }
    ],
    "interactiveChecks": [
      {
        "action": "Inspected the repo root state tree after the rename.",
        "observed": ".cogent exists, .cagent is gone, and the tracked artifact subtree is intact."
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "sandbox/src/self_directed_dispatch.rs",
        "cases": [
          {
            "name": "dispatch shellouts use cogent commands",
            "verifies": "The runtime command strings and conflict handling match the renamed CLI surface."
          }
        ]
      }
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- The feature requires changing `~/cogent` or any other out-of-scope external repo
- The feature would require ATLAS removal rather than simple rename fallout fixes
- A validator fails for a reason that is clearly unrelated to the feature and blocks progress
- The exact active-doc scope is ambiguous enough that continuing would risk archive/history churn
