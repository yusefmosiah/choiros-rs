# Secrets Architecture Audit & Scaling Plan

Date: 2026-03-12
Kind: Report
Status: Active
Priority: 1
Requires: [adr-0008, adr-0020, adr-0003]

## Narrative Summary (1-minute read)

The hypervisor crashed after reboot because `openrouter_api_key` was deployed
to tmpfs but not persistent storage. systemd's `LoadCredential` hard-fails
when a declared credential file is missing — correct behavior, but it reveals
that we have **multiple overlapping secrets delivery strategies** with gaps
between them.

This audit catalogs all current approaches, maps them against the ADR roadmap,
and proposes a path to per-user shielded secrets that actually scales.

## What Changed

Nothing changed in code — this is an analysis document. The immediate fix
(persisting the OpenRouter key to `/opt/choiros/secrets/platform/`) was
applied operationally.

## What To Do Next

1. Fix the LoadCredential crash class (make missing optional keys non-fatal)
2. Implement ADR-0020 Phase 1 quick wins (admin auth, secure cookie, non-root sandbox)
3. Implement per-VM HMAC tokens (ADR-0020 Phase 2)
4. Build per-user secrets broker (ADR-0003/0008 Phase C)

---

## The Bug That Triggered This Review

```
hypervisor.service: Failed to set up credentials: No such file or directory
hypervisor.service: Failed at step CREDENTIALS
```

`LoadCredential=` is an **all-or-nothing** directive. If ANY declared credential
file is missing, systemd refuses to start the service. This is by design —
systemd treats `LoadCredential` as a hard dependency. But our architecture
treats provider keys as **optional capabilities**, not hard requirements. A
missing OpenRouter key should degrade that provider, not kill the hypervisor.

### Fix options

| Option | Approach | Trade-off |
|--------|----------|-----------|
| A. `LoadCredentialEncrypted` | Encrypted fallback | Adds complexity, still hard-fails if missing |
| B. `SetCredentialEncrypted` | Inline default | Leaks dummy values into unit file |
| **C. Split required/optional** | Two service configs | Clean, matches actual semantics |
| D. Read from persistent path | Skip LoadCredential for optional keys | Bypasses systemd sandboxing |

**Recommended: Option C.** Split credentials into:
- `LoadCredential` for **required** keys (provider_gateway_token, aws_bedrock)
- Read from `$CREDENTIALS_DIRECTORY` at runtime, gracefully degrade for **optional** provider keys

The provider gateway code already handles missing keys gracefully (returns 503
per-request). The issue is purely at the systemd level.

---

## Current Secrets Delivery Methods (5 strategies in use)

### 1. NixOS LoadCredential (hypervisor platform keys)

**How:** Persistent NVMe → boot-time materialize service → tmpfs → LoadCredential
**Scope:** 10 provider API keys + gateway token
**Failure mode:** Hard crash if ANY file missing
**Security:** Best practice — keys in `$CREDENTIALS_DIRECTORY`, not `/proc/pid/environ`

### 2. EnvironmentFile (hypervisor non-secrets)

**How:** Materialized from persistent storage, sourced by systemd
**Scope:** Ports, DB URL, paths
**Failure mode:** Hard crash if file missing
**Security:** Acceptable for non-secrets, but `hypervisor.env` shares the same
delivery path as actual secrets, blurring the boundary

### 3. Kernel cmdline injection (sandbox gateway token)

**How:** Hypervisor writes token to state file → systemd injects into `--cmdline`
→ guest extracts from `/proc/cmdline` → writes to tmpfs env file
**Scope:** Single static gateway token per VM
**Failure mode:** Sandbox can't reach provider gateway
**Security:** **Weak.** `/proc/cmdline` is world-readable. ADR-0020 C2 flags this.

### 4. EventStore persistence (user preferences)

**How:** User model config stored as events, queried at runtime
**Scope:** Theme, model selection per callsite
**Failure mode:** Falls back to defaults
**Security:** Not for secrets — EventStore events are observable/streamable

### 5. SQLite + WebAuthn (user identity)

**How:** Passkeys stored as JSON in hypervisor DB, sessions in SQLite
**Scope:** User authentication, session management
**Failure mode:** 401 unauthorized
**Security:** Modern FIDO2, no passwords, Argon2id recovery codes

---

## Gap Analysis: What's Missing for Per-User Shielded Secrets

### ADR-0003 (Draft) defines the target but isn't implemented:

| Component | Status | Blocking? |
|-----------|--------|-----------|
| `user_secrets` table | ❌ Missing | Yes — no storage layer |
| `PUT /me/secrets/:name` | ❌ Missing | Yes — no CRUD API |
| `DELETE /me/secrets/:name` | ❌ Missing | Yes — no CRUD API |
| Internal broker resolution | ❌ Missing | Yes — no runtime secret delivery |
| Encryption at rest | ❌ Missing | Yes — user secrets need per-user encryption |
| Audit metadata (no raw values) | ❌ Missing | Yes — compliance requirement |
| Secret rotation API | ❌ Missing | No — can defer |

### ADR-0008 Phase C (per-user broker) requires:

1. **Database migration:** `user_secrets` table with `(user_id, secret_name, encrypted_value, created_at, updated_at)`
2. **Encryption key management:** Per-user envelope encryption (data key encrypted by user's master key, master key derived from... what?)
3. **Broker endpoint:** Internal-only `/broker/resolve` that takes `(user_id, capability)` and returns secret value
4. **Policy engine:** Which capabilities map to which secrets for which users
5. **Audit trail:** Every resolve logged with `(user_id, capability, timestamp, sandbox_id)` but NOT the secret value

### ADR-0020 (Accepted) must come first:

Per-user secrets are pointless without per-VM isolation. ADR-0020 Phase 2
(per-VM HMAC tokens) establishes the trust chain:

```
User authenticates (WebAuthn)
  → Hypervisor provisions VM with HMAC token
  → VM presents HMAC token to gateway
  → Gateway verifies: HMAC(master, user_id:sandbox_id)
  → Gateway resolves user's secrets via broker
  → Secret delivered to sandbox for single request
  → Secret never persisted in VM
```

Without per-VM tokens, any VM can impersonate any user at the gateway.

---

## Scaling Plan: From Current State to Per-User Shielded Secrets

### Phase 0: Fix the crash class (immediate, ~1 hour)

Split `LoadCredential` in `ovh-node.nix`:
- Required: `provider_gateway_token`, `aws_bedrock`, `hypervisor.env`
- Optional: All other provider keys — read from persistent path at runtime
  with graceful 503 per-provider on missing

Update `read_secret_env_or_credential()` in `provider_gateway.rs` to also
check `/opt/choiros/secrets/platform/<name>` as a fallback path when
`$CREDENTIALS_DIRECTORY/<name>` is missing.

### Phase 1: ADR-0020 quick wins (~2 hours)

1. Admin auth: `CHOIR_ADMIN_USER_IDS` allowlist for admin endpoints
2. Secure cookie: `with_secure(true)` behind env flag
3. Sandbox non-root: `User=choiros` in guest VM service
4. Hypervisor hardening: `NoNewPrivileges`, `ProtectHome`, `PrivateTmp`
5. Token file permissions: mode 0600 after writing

### Phase 2: Per-VM HMAC tokens (~4 hours)

Replace static gateway token with per-VM HMAC:
```
token = HMAC-SHA256(master_key, user_id + ":" + sandbox_id)
```

Gateway verifies token, extracts user_id from claims.
Remove self-reported `x-choiros-user-id` header trust.

### Phase 3: Network isolation (~2 hours)

Replace `trustedInterfaces = ["br-choiros"]` with nftables:
- Block VM→VM traffic
- Allow VM→host:9090 (gateway) + DHCP only
- Allow outbound NAT for internet access

### Phase 4: User secrets broker (~8 hours)

1. DB migration: `user_secrets` table
2. API: `PUT/DELETE /me/secrets/:name` (authenticated, encrypted at rest)
3. Broker: Internal `/broker/resolve` endpoint
4. Envelope encryption: Data key per secret, master key per user
   (derived from user_id + platform master secret via HKDF)
5. Audit events: `EVENT_SECRET_RESOLVED`, `EVENT_SECRET_CREATED`,
   `EVENT_SECRET_DELETED` (metadata only, no values)
6. Policy: Hardcoded initial mapping (e.g., `github_token` → git tools,
   `openai_key` → model override)

### Phase 5: virtio-vsock secret delivery (~4 hours)

Replace kernel cmdline token injection with virtio-vsock channel:
- Host opens vsock listener per VM
- Sandbox connects to host vsock on boot
- Host delivers signed token + per-request secrets over vsock
- No `/proc/cmdline` exposure
- Enables streaming secret rotation without VM restart

---

## ADR Dependencies and Ordering

```
ADR-0020 Phase 1 (quick wins)
  ↓
ADR-0020 Phase 2 (per-VM HMAC tokens)
  ↓ requires trust chain for...
ADR-0003/0008 Phase C (user secrets broker)
  ↓ requires isolation for...
ADR-0020 Phase 3 (network isolation)
  ↓ hardens...
ADR-0020 Phase 4 (vsock delivery)

Independent:
  ADR-0024 (Go rewrite) — must PRESERVE all boundaries above
  ADR-0014 (build pool) — uses same VM isolation primitives
```

## ADR-0024 Implications

The Go rewrite explicitly states it must preserve all trust boundaries. The
secrets architecture is hypervisor-side, so if the hypervisor is rewritten
in Go, the entire LoadCredential + provider gateway + broker system must be
re-implemented. The recommended approach from ADR-0024 is decomposition:
extract the provider gateway as a standalone service first, then the secrets
broker, then the auth edge. Each extracted component keeps its contract.

This actually HELPS the secrets architecture — a standalone secrets broker
service is cleaner than embedding it in the hypervisor monolith. The Go
rewrite creates a natural extraction point for each security boundary.

---

## The pmem/KSM Security Demonstration (Future)

The per-user shielded secrets architecture enables a powerful security demo:

1. **What leaks (pmem):** Shared nix store cache timing → tool execution patterns
2. **What doesn't leak (blk + broker):** User data, secrets, file contents, prompts
3. **What's shielded (broker):** User API keys never touch VM disk, delivered
   per-request via vsock, audit-logged, time-bounded

The demo script:
- Attacker VM runs Flush+Reload on nix store paths
- Victim VM runs conductor prompt (triggers tool execution)
- Attacker sees temporal correlation (which tools ran)
- Attacker tries same on user data paths → no signal
- Attacker tries to intercept gateway requests → HMAC token fails
- Side-by-side video: "Here's what infrastructure sees. Here's what it can't."

This is the trust transparency statement: we don't hide the side channels,
we enumerate them and show what's protected.
