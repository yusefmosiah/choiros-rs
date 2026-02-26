# 2026-02-22 MVP Release and Documentation Stocktake

Date: 2026-02-22
Status: in progress (grind build/promote running for `4a57057`)
Owners: platform + runtime

## Narrative Summary (1-minute read)

Today shifted ChoirOS from ad-hoc key handling and host drift toward a deterministic
"build on grind, promote exact closures to prod" flow. The runtime now routes provider
calls through the hypervisor gateway, sandbox key exposure is reduced, and grind is the
active iteration domain (`choir.chat`) while prod remains `choir-ip.com`.

The immediate product state is "MVP works, but reliability/documentation lagged behind
velocity." This stocktake records what shipped today, what still blocks confidence, and
what to do next.

## What Changed

### 1. Deploy and release path hardening

- Added/updated release automation and runbooks for Mac SSH-first promotion:
  - `scripts/ops/build-release-manifest.sh`
  - `scripts/ops/apply-release-manifest.sh`
  - `scripts/ops/promote-grind-to-prod.sh`
  - `docs/runbooks/mac-ssh-release-flow.md`
  - `docs/runbooks/grind-to-prod-release-flow.md`
- Promotion now enforces clean grind tree by default and moves exact Nix closures to prod.

### 2. Keyless sandbox + provider gateway safety work

- Hypervisor/provider gateway now supports required auth header handling and improved logs.
- Sandbox runtime env hardened to avoid passing high-sensitivity provider secrets directly.
- Bedrock traffic routing moved behind provider gateway path without provider rewrite hacks.
- Grind/prod host config updated to pass gateway metadata (`CHOIR_SANDBOX_*`) needed for
  policy/rate-limit identity.

### 3. Runtime/model routing updates

- `sandbox/config/model-catalog.toml` callsite defaults now:
  - `terminal = "KimiK25"`
  - `conductor = "KimiK25"`
  - `writer = "KimiK25"`
  - `researcher = "ZaiGLM47"`
- Commit: `4a57057` (`config: default terminal/writer/conductor to kimi and researcher to glm47`).

### 4. Frontend/runtime stabilization progress

- Public login path returns 200 on both environments.
- Desktop load/proxy execution failures were narrowed into gateway/policy/config gaps and
  partially resolved on grind.
- Apex domain behavior differs from `os.*` subdomain behavior and remains an explicit
  operations check item during release validation.

### 5. Documentation growth but fragmentation

- Significant new handoffs/runbooks were added during active incident+ship work.
- The "single coherent day narrative" was missing; this stocktake fills that gap.

## Current Goal (Re-stated)

1. Stabilize MVP runtime loop (login -> desktop load -> prompt execution) on grind.
2. Promote only known-good grind commits to prod via closure promotion.
3. Resume marginalia/writer UX development on top of stable runtime behavior.

## Key Risks Right Now

- Drift risk: host-level config changes can outpace repo docs unless captured immediately.
- Build contention risk: overlapping grind builds can leave stale/redundant jobs and delay release.
- UX risk: writer/marginalia scope can expand before core runtime reliability is consistently green.
- Domain confusion risk: `choir.chat` (grind) vs `choir-ip.com` (prod) can invalidate test assumptions.

## What To Do Next

1. Complete current promotion for `4a57057`, then verify prod health and basic prompt execution.
2. Add one short "runtime model override" operator doc that defines safe runtime override paths
   (config/API), so model switching is explicit and not prompt-driven.
3. Create a focused marginalia phase checkpoint doc (what is done vs missing in Phase 1.5).
4. Add a release checklist line item to always capture:
   - grind commit SHA
   - promoted SHA
   - domain tested
   - prompt execution result
5. Start next MVP slice only after the above is green on both grind and prod.

## Appendix: 2026-02-22 Commit Stream Snapshot

Latest first:

- `4a57057` config: default terminal/writer/conductor to kimi and researcher to glm47
- `cea831e` Route aws-bedrock through provider gateway without provider rewrite
- `8d879e6` hypervisor: improve provider gateway token header parsing/logging
- `e90cbbc` hypervisor: accept x-api-key token for provider gateway auth
- `a7479bf` hypervisor: set HOME/cache defaults for keyless sandbox runtime
- `13b19bc` hypervisor: explicitly pass FRONTEND_DIST to sandbox child
- `b73a68c` hypervisor: pass FRONTEND_DIST to process sandboxes
- `aa93235` Harden sandbox env isolation and release promotion scripts
- `ab07c5d` Add Mac SSH-only release runbook
- `2c77eb7` Add Nix 101 and closure promotion diagram to release runbook
- `cbbd422` Add FlakeHub cache check and fail-fast deploy fallback
- `e1e2bad` Update ops docs, restore auth routes, and harden FlakeHub login unit
- `321d490` Add FLAKEHUB_AUTH_TOKEN to encrypted platform secrets
- `b15569e` Rename FlakeHub secret key to FLAKEHUB_AUTH_TOKEN
- `84cf4f6` Add declarative FlakeHub token login via sops-nix
- `d7b5cb8` Rework local runtime routing for sandbox-owned UI
- `423aa4e` pre hypervisor sandbox ui boundary
- `344cd50` Adjust frontend API base logic
- `d277d7e` Retry core app registration
- `3ac5c65` pre sandbox loading
- `e1a56db` fix desktop bootstrap to wait for auth and improve HTML redirect errors
- `8516b24` rename sandbox-ui to dioxus-desktop across codebase
- `620ef33` docs: handoff tls/auth fix and remaining sandbox load issue on grind
- `d354cc9` add mac-driven e2e handoff and prod reset plan
- `3d8c695` add deterministic grind-to-prod release runbook
- `4093bf6` Trigger deploy pipeline when deploy scripts change
- `b907455` Add host build fallback when SSM store paths are unavailable
- `e3b9018` Fix deploy job by checking out host switch script
- `146558d` Harden keyless gateway path and codify AWS deploy contract
- `2645bb6` pre key gateway
- `5708563` pre key gateway
- `f0d9d84` Skip CI builds when deploy inputs are unchanged
- `451bfed` Run sandbox health probes inside containers

