# Handoff: Sandbox Key + Gateway 80/20

Date: 2026-02-22  
Owner: runtime/security  
Status: in progress (root cause confirmed, code patch ready, deploy pending)

## Narrative Summary (1-minute read)

Sandbox startup failure was caused by provider API keys leaking from the `hypervisor`
process environment into spawned sandbox child processes. Sandbox is correctly enforcing
keyless policy and exits when it detects provider credentials. We have now patched the
spawn path to clear inherited environment variables and pass only an explicit allowlist.

In parallel, local memory was simplified to symbolic retrieval only (no local ONNX/
embedding runtime), aligning with the product direction to defer vector infrastructure
to a future global/publishing layer.

The immediate 80/20 path is:
1) keep sandbox keyless,
2) route model access through gateway token only,
3) enforce minimal gateway policy and logging.

## What Changed

1. Root cause confirmed for key leakage:
   - `hypervisor` service loads secrets via `EnvironmentFile` from
     `nix/modules/choiros-platform-secrets.nix`.
   - spawn code in `hypervisor/src/sandbox/mod.rs` previously inherited env by default.
2. Spawn hardening patch applied:
   - `hypervisor/src/sandbox/mod.rs` now uses `env_clear()` before launching sandbox.
   - Minimal safe allowlist re-injected (`PATH`, locale/TLS/log vars, etc.).
   - Explicit sandbox vars still passed (`PORT`, `DATABASE_URL`, `SQLX_OFFLINE`).
3. ONNX/local-embedding simplification completed:
   - `sandbox/src/actors/memory.rs` converted to symbolic lexical retrieval.
   - Removed local embedding/ONNX dependency path in active memory actor.
   - `sandbox/Cargo.toml` removed `fastembed`, `sqlite-vec`, `zerocopy`.
   - `sandbox/tests/memory_actor_test.rs` updated and passing.
4. Local validation completed:
   - `cargo check -p sandbox` passed.
   - `cargo check -p hypervisor` passed.
   - `./scripts/sandbox-test.sh --test memory_actor_test` passed (11/11).

## Current Security/Runtime Posture

- Sandbox keyless enforcement exists in `sandbox/src/main.rs` and is working as intended.
- Hypervisor still has platform keys available from host secrets (by design currently).
- Child sandbox key leakage is blocked by spawn env clearing patch (deploy needed).
- Provider access strategy should move to gateway-token-only from sandbox.

## What To Do Next

1. Deploy the hypervisor env-clearing patch and verify in prod that sandbox no longer
   receives provider key env vars.
2. Ensure sandbox receives only gateway credentials required for proxied calls:
   - `CHOIR_PROVIDER_GATEWAY_BASE_URL`
   - `CHOIR_PROVIDER_GATEWAY_TOKEN` (short-lived preferred)
3. Keep provider key material in hypervisor/gateway boundary only.
4. Add minimal gateway enforcement if missing:
   - token validation,
   - upstream allowlist,
   - basic per-sandbox rate limit key.
5. Add structured gateway usage logs for observability and future billing/rate control.

## 1-Hour Implementation Checklist (80/20)

### 0:00-0:10 — Lock sandbox env contract

- Confirm `env_clear()` + allowlist remains in `hypervisor/src/sandbox/mod.rs`.
- Pass only proxy values into sandbox env (`CHOIR_PROVIDER_GATEWAY_*`).
- Do not pass provider keys to sandbox.

### 0:10-0:20 — Wire gateway values

- Verify `hypervisor/src/config.rs` has gateway base/token config.
- Ensure sandbox spawn sets gateway env vars from hypervisor config.
- Keep sandbox forbidden-key checks intact.

### 0:20-0:30 — Minimal gateway policy

- In `hypervisor/src/provider_gateway.rs`, enforce:
  - valid proxy token,
  - allowed upstream base URLs,
  - basic per-sandbox rate key.

### 0:30-0:40 — Observability

- Add structured logs per proxied call:
  - `sandbox_id`, `user_id`, `provider`, `model`, `status`, `latency_ms`.
- Avoid logging sensitive payloads or secrets.

### 0:40-0:50 — Validation

- Run:
  - `cargo check -p hypervisor`
  - `cargo check -p sandbox`
- Smoke verify:
  - sandbox starts without provider env keys,
  - proxy call succeeds,
  - direct provider access path from sandbox is not used.

### 0:50-1:00 — Deploy and verify

- Deploy patch and restart services.
- Verify runtime:
  - no provider keys in sandbox env,
  - desktop connection no longer hangs at startup due to sandbox crash,
  - gateway calls visible in logs.

## File Pointers

- `hypervisor/src/sandbox/mod.rs`
- `hypervisor/src/config.rs`
- `hypervisor/src/provider_gateway.rs`
- `sandbox/src/main.rs`
- `sandbox/src/actors/memory.rs`
- `sandbox/tests/memory_actor_test.rs`
- `nix/modules/choiros-platform-secrets.nix`
