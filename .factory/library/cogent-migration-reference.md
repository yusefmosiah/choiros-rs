# Cogent Migration Reference

## Context

The cogent project (formerly "cagent") at ~/cogent was renamed and refactored:
- GitHub: `github:yusefmosiah/cogent` (old URL `github:yusefmosiah/cagent` redirects)
- Binary: `cogent` (was `cagent`)
- Go module: `github.com/yusefmosiah/cogent`
- Data directory: `.cogent/` (was `.cagent/`)
- Database files: `cogent.db`, `cogent-private.db`
- Available adapters: `claude`, `native` only (codex, factory, pi, gemini, opencode all removed)

## Current Mission Boundary

This mission is a **hard cutover in `choiros-rs` only**.

- Do **not** change `~/cogent`
- Do **not** add `.cagent` fallback support there
- Cut over the repo-local state in `choiros-rs` from `.cagent/` to `.cogent/`
- Remove `supervisor.json` instead of renaming/preserving it

## Complete Rename Inventory in choiros-rs

### Rust Source Code (MUST change)
- `sandbox/src/self_directed_dispatch.rs:155` — `Command::new("cagent")` → `Command::new("cogent")`
- `sandbox/src/self_directed_dispatch.rs:99,123,136,357,363,372,376` — string literals referencing "cagent"

### Nix Files (MUST change)
- `flake.nix:16-17` — input URL: `github:yusefmosiah/cagent` → `github:yusefmosiah/cogent`
- `flake.nix:22` — input parameter name: `cagent-src` → `cogent-src`
- `flake.nix:150` — `cagentPackage` → `cogentPackage`
- `flake.nix:360-365` — buildGoModule: pname, src, subPackages `cmd/cagent` → `cmd/cogent`
- `nix/ch/sandbox-vm.nix:5,253,320` — `cagentPackage` → `cogentPackage`
- `nix/ch/sandbox-vm.nix:168,244` — comments
- `nix/ch/sandbox-vm.nix:315` — remove `codex` from guest packages

### Data Directory (MUST change)
- `.cagent/` → `.cogent/` (git mv for tracked files)
- `.cagent/cagent.db` → `.cogent/cogent.db`
- `.cagent/cagent-private.db` → `.cogent/cogent-private.db`
- remove `.cagent/supervisor.json`
- Update `.gitignore` entries

### Documentation (MUST change)
- `CLAUDE.md` — all cagent CLI examples, codex adapter reference
- `README.md` — cagent references, codex/claude architecture
- `AGENTS.md` — will be replaced by mission AGENTS.md
- `docs/cagent-spec-and-implementation-guide.md` — 100+ references, env vars CAGENT_* → COGENT_*
- `docs/adr-0029-cagent-vsock-work-broker.md` — 30+ references
- `docs/adr-0024-hypervisor-go-rewrite.md` — cagent references
- `docs/adr-0026-implementation.md` — cagent references
- `docs/adr-0026-self-directing-agent-dispatch.md` — cagent references
- `docs/state-report-*.md` — cagent references
- `docs/note-*.md` — cagent references
- `docs/ATLAS.md` — generated, will be regenerated

### Config/Scripts (MUST change)
- `.githooks/pre-commit` — cagent CLI invocations
- `scripts/ops/validate-local-provider-matrix.sh` — codex-openai-bridge logic (remove or update)

### Config Files to Remove/Update
- `opencode.json` — remove (opencode adapter stripped)
- `opencode.json.backup` — remove
- `.gitignore:89` — `.codex/` entry (remove)
- `.gitignore:106` — `.opencode/` entry (remove)

### Comments Only (low priority)
- `sandbox/src/actors/terminal.rs:1,4,17` — "opencode integration" comments → update or remove
