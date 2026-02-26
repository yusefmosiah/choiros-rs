# Handoff: Local CI/CD Iteration First

Date: 2026-02-21  
Owner: runtime/deploy  
Status: in progress (pipeline scaffold merged; green run not achieved yet)

## Narrative Summary (1-minute read)

We wired a full Nix CI/CD workflow (build matrix, Playwright domain gate, EC2 deploy),
migrated deploy secrets to `EC2_*`, and fixed two blocking classes of failures discovered
from live GitHub runs: missing lockfiles in flake source and missing OpenSSL/pkg-config in
Nix builds.

We now need to stop iterating by push-only runs and move to local-first validation until
checks pass, then push once.

## What Changed

1. CI/CD workflow added in `.github/workflows/nix-ci-draft.yml`:
   - Nix builds for `sandbox`, `hypervisor`, `dioxus-desktop`
   - Playwright domain smoke gate (`bios-auth.spec.ts`, `proxy-integration.spec.ts`)
   - EC2 deploy stage with rollback summary
2. Secrets migrated to canonical names and old ones removed:
   - Added: `EC2_HOST`, `EC2_USER`, `EC2_SSH_KEY`, `EC2_SSH_PORT`
   - Removed: `SSH_HOST`, `SSH_USER`, `SSH_KEY`, `SSH_KNOWN_HOSTS`, `DEPLOY_PATH`
3. Lockfile and source fixes:
   - Tracked `Cargo.lock` and `dioxus-desktop/Cargo.lock`
   - Updated flake source filters to include lockfiles
4. Nix dependency wiring fixes:
   - Added `nativeBuildInputs = [ pkg-config ]` and `buildInputs = [ openssl ]`
     in `sandbox/flake.nix` and `hypervisor/flake.nix`
5. Desktop flake path fix for workspace-local dependency:
   - `dioxus-desktop/flake.nix` now builds from repo root source and uses
     `--manifest-path dioxus-desktop/Cargo.toml`

## Current Remote Run Snapshot

- In progress: `Nix CI/CD` run `22253294668` (triggered by commit `bd14e8f`)
- Earlier failures observed and addressed:
  - missing `Cargo.lock`
  - missing `pkg-config`/OpenSSL for `openssl-sys`
  - desktop flake path issue for `shared-types`

## What To Do Next

1. Run full local CI-equivalent loop before any additional push.
2. Only after local pass, push once and verify one clean `Nix CI/CD` run.
3. If green, proceed to deploy stage validation and EC2 runtime smoke verification.

## Local Iteration Checklist

Use this order from repo root:

```bash
# 1) Rust baseline checks
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test -p sandbox --verbose

# 2) Nix flake builds (requires nix installed locally)
nix build ./sandbox#sandbox --print-build-logs
nix build ./hypervisor#hypervisor --print-build-logs
nix build ./dioxus-desktop#desktop --print-build-logs

# 3) Dioxus web asset build used by deploy job
nix develop ./dioxus-desktop --command dx build --release

# 4) Playwright domain smoke gate
cd tests/playwright
npm ci
npx playwright install --with-deps chromium
PLAYWRIGHT_HYPERVISOR_BASE_URL=https://os.choir-ip.com \
  npx playwright test --project hypervisor bios-auth.spec.ts proxy-integration.spec.ts
```

## Operational Logs to Watch During Deploy Validation

```bash
# GitHub Actions
gh run list --repo yusefmosiah/choiros-rs --limit 10
gh run watch <run-id> --repo yusefmosiah/choiros-rs --exit-status
gh run view <run-id> --repo yusefmosiah/choiros-rs --log-failed

# EC2 runtime logs
ssh -i ~/.ssh/choiros-production.pem root@54.211.83.193 "journalctl -u hypervisor -f"
ssh -i ~/.ssh/choiros-production.pem root@54.211.83.193 "journalctl -u container@sandbox-live -f"
ssh -i ~/.ssh/choiros-production.pem root@54.211.83.193 "journalctl -u container@sandbox-dev -f"
ssh -i ~/.ssh/choiros-production.pem root@54.211.83.193 "journalctl -u caddy -f"

# EC2 control plane events
aws cloudtrail lookup-events \
  --region us-east-1 \
  --lookup-attributes AttributeKey=ResourceName,AttributeValue=i-0cb76dd46cb699be6 \
  --max-results 20
```
