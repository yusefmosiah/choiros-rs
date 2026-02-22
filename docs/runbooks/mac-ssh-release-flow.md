# Mac SSH Release Flow (No AWS)

## Narrative Summary (1-minute read)

This runbook keeps deploy control on your Mac and uses only SSH + Nix closures.
You do not need AWS CLI, AWS SSO, or SSM for this path.

Flow:
1. Commit and push from grind.
2. Pull on your Mac.
3. Run one promote command from Mac.
4. Verify public health/login.

Promotion now enforces clean `main` on both hosts, builds only from a clean tree on grind,
copies exact store closures to prod, then applies them on prod.
That avoids surprise recompiles and ensures binaries are actually updated from the release manifest.

## What Changed

1. Documented a Mac-first, SSH-only promotion path.
2. Added exact commands for preflight, promotion, and verification.
3. Added quick failure handling for the common blockers.

## What To Do Next

1. Configure SSH aliases/keys on Mac for grind and prod.
2. Pull latest `main` on Mac.
3. Run the promote command in this doc.
4. If promote fails, follow the failure section and retry.

## Preconditions

1. Mac has `nix` with flakes enabled.
2. Mac can SSH to both hosts:
   - grind: `root@18.212.170.200`
   - prod: `root@172.31.93.29`
3. Both hosts have repo at `/opt/choiros/workspace`.
4. `origin/main` is up to date and deployable.
5. Any uncommitted host changes are committed/stashed first (dirty trees are rejected).

## Domain/TLS Note (Current State)

Host Caddy config still lives in `/etc/nixos/configuration.nix` on each host (not yet tracked in this repo).
Keep host vhosts aligned manually until host config is moved into repo/flake outputs.

Current grind expectation:
- `os.choir.chat` -> `reverse_proxy 127.0.0.1:9090`
- `choir.chat` -> `redir https://os.choir.chat{uri} 308`

## One-Time SSH Setup On Mac

Add to `~/.ssh/config`:

```sshconfig
Host choiros-grind
  HostName 18.212.170.200
  User root
  IdentityFile ~/.ssh/<your-grind-key>
  IdentitiesOnly yes
  StrictHostKeyChecking accept-new

Host choiros-prod
  HostName 172.31.93.29
  User root
  IdentityFile ~/.ssh/<your-prod-key>
  IdentitiesOnly yes
  StrictHostKeyChecking accept-new
```

Quick check:

```bash
ssh choiros-grind 'echo ok-grind'
ssh choiros-prod 'echo ok-prod'
```

## Standard Release From Mac

From your Mac repo checkout:

```bash
git fetch --all --prune
git checkout main
git pull --ff-only

./scripts/ops/promote-grind-to-prod.sh \
  --grind choiros-grind \
  --prod choiros-prod \
  --repo /opt/choiros/workspace \
  --manifest /tmp/choiros-release.env
```

What the command does:
1. Syncs grind repo to `origin/main` and fails if still dirty.
2. Syncs prod repo to `origin/main` and fails if still dirty.
3. Verifies both hosts are on the same commit SHA.
4. Builds flake outputs on grind and writes manifest (dirty build blocked).
5. Copies exact closure paths grind -> prod via `nix copy`.
6. Applies release manifest on prod (updates `/opt/choiros/bin/*` symlinks + restarts services).
7. Runs health checks in `apply-release-manifest.sh`.

## Post-Deploy Verify

```bash
curl -I https://os.choir.chat/login
curl -fsS https://os.choir.chat/health || true
```

Optional deeper checks:

```bash
ssh choiros-prod 'systemctl status hypervisor --no-pager'
ssh choiros-prod 'journalctl -u hypervisor -n 120 --no-pager'
```

## Common Failures

Dirty tree on grind:

```bash
ssh choiros-grind 'cd /opt/choiros/workspace && git status --short --branch'
```

Fix by committing/stashing before retry.

Dirty tree on prod:

```bash
ssh choiros-prod 'cd /opt/choiros/workspace && git status --short --branch'
```

Fix by committing/stashing before retry.

SSH auth failure (`Permission denied (publickey)`):
1. Confirm correct key path in `~/.ssh/config`.
2. Confirm matching public key is in target host `~/.ssh/authorized_keys`.
3. Retry `ssh choiros-grind` / `ssh choiros-prod`.

Cache uncertainty:
1. On grind, run `./scripts/ops/check-flakehub-cache.sh`.
2. Ensure substituter + netrc are configured before heavy builds.

## Event-Day Minimal Path

If time is tight before demo:
1. Keep changes small.
2. Push to `main`.
3. Run promote command above.
4. Verify `/login` returns `200`.
