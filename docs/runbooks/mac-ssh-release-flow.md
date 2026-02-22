# Mac SSH Release Flow (No AWS)

## Narrative Summary (1-minute read)

This runbook keeps deploy control on your Mac and uses only SSH + Nix closures.
You do not need AWS CLI, AWS SSO, or SSM for this path.

Flow:
1. Commit and push from grind.
2. Pull on your Mac.
3. Run one promote command from Mac.
4. Verify public health/login.

Promotion copies exact store closures from grind to prod, then applies them on prod.
That avoids surprise recompiles and keeps prod aligned with what was built on grind.

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
4. Repo tree on grind is clean before release build.

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
1. Builds flake outputs on grind and writes manifest.
2. Copies exact closure paths grind -> prod via `nix copy`.
3. Applies release manifest on prod (installs bins + `nixos-rebuild switch`).
4. Runs post-switch health checks in host switch script.

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
