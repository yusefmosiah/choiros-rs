# Handoff: AWS NixOS Containers Pivot

Date: 2026-02-21  
Owner: runtime/deploy  
Status: in progress (native containers + public domain/TLS live; auth onboarding fixed)

## Narrative Summary (1-minute read)

We pivoted deployment direction from OCI/Podman-first to native NixOS containers
(`containers.<name>`) on an AWS NixOS EC2 host. This keeps the architecture lightweight
for MVP while staying fully declarative and ready to migrate to microVMs later.

Documentation was updated to make NixOS containers the default path for Phase 1 and
AWS rollout gates.

## What Changed

1. Updated `docs/architecture/2026-02-20-nixos-aws-sandbox-deployment-runbook.md`
   to use NixOS containers as the runtime contract.
2. Updated `docs/architecture/2026-02-20-bootstrap-execution-checklists.md` with
   NixOS container substrate tasks/evidence.
3. Updated `docs/runbooks/nix-setup.md` Phase 3 from Podman runtime to native
   `containers.sandbox-live`/`containers.sandbox-dev` examples.

## AWS Runtime State

- Region: `us-east-1`
- NixOS AMI owner: `427812963091`
- Instance: `i-0cb76dd46cb699be6`
- Public IP: `54.211.83.193`
- Security Group: `sg-03f8fe76f447270db`
- SSH key used: `/Users/wiz/.ssh/choiros-production.pem`

Current host state:
- NixOS host is reachable and rebuilt with native containers enabled.
- `containers.sandbox-live` and `containers.sandbox-dev` are active and healthy.
- Sandbox binary is built on-host and installed at `/opt/choiros/bin/sandbox`.
- Health checks pass on public ports:
  - `http://54.211.83.193:8080/health` (live)
  - `http://54.211.83.193:8081/health` (dev)
- Hypervisor is running as a host systemd service on `:9090` and serves the
  desktop/auth shell at `http://54.211.83.193:9090/login`.
- Hypervisor is configured to serve frontend assets from:
  `/opt/choiros/workspace/dioxus-desktop/target/dx/sandbox-ui/release/web/public`.
- Hypervisor sandbox registry now adopts existing listeners on configured
  live/dev ports, which allows host-managed NixOS containers to remain the
  runtime substrate.
- Container private health endpoints also pass:
  - `http://10.233.1.2:8080/health`
  - `http://10.233.2.2:8080/health`
- Important runtime note: for MVP operability we set `privateUsers = "no"` because
  `privateUsers = "pick"` produced unwritable bind-mounted `/data` volumes for
  SQLite startup.

## Domain and TLS State

- Public app domain is now active at `https://os.choir-ip.com` via Caddy on the EC2 host.
- Apex domain `https://choir-ip.com` is configured and redirects to
  `https://os.choir-ip.com` with `308`.
- Hypervisor WebAuthn env now uses:
  - `WEBAUTHN_RP_ID=os.choir-ip.com`
  - `WEBAUTHN_RP_ORIGIN=https://os.choir-ip.com`
- Caddy reverse proxy target remains `127.0.0.1:9090` (hypervisor).

## Verification Snapshot

- Public domain checks:
  - `http://os.choir-ip.com/login` -> `308` to HTTPS
  - `https://os.choir-ip.com/login` -> `200`
  - `https://choir-ip.com/login` -> `308` to `https://os.choir-ip.com/login`
- Playwright `hypervisor` project passed against public HTTPS domain:
  - `tests/playwright/bios-auth.spec.ts`
  - `tests/playwright/proxy-integration.spec.ts`
  - Result: 12/12 tests passed after auth UX update.

## Immediate Next Steps

1. Validate hypervisor route/swap behavior against live/dev targets.
2. Decide and codify the canonical binary delivery strategy for
   `/opt/choiros/bin/sandbox` (host-native flake build preferred).
3. Revisit user namespace hardening (`privateUsers = "pick"`) with an idmapped
   mount plan that preserves `/data` write permissions.
4. Tighten SG and host firewall to least privilege (operator CIDRs/private access).
5. Implement CI/CD with cache-backed Nix builds and production deployment automation.

## Notes

- We explicitly accept lightweight isolation for MVP.
- Stronger isolation (microVMs) remains deferred and should be introduced as a
  subsequent hardening phase without changing the hypervisor route/swap contract.
