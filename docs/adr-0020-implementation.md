# Implementing ADR-0020: Security Hardening

Date: 2026-03-10
Kind: Guide
Status: Active
Priority: 1
Requires: [ADR-0020]

## Narrative Summary (1-minute read)

Security architecture review found 3 critical and 5 high findings in
the application and configuration layers. The VM hypervisor boundary is
sound. This guide covers implementation of fixes in priority order,
grouped into phases that can be deployed independently.

## What Changed

- 2026-03-10: Initial implementation guide.

## What To Do Next

Start with Phase 1 (quick wins), then Phase 2 (per-VM tokens), then
Phase 3 (network isolation). Each phase is independently deployable.

---

## Phase 1: Quick Wins (1-2 hours)

### 1a. Admin Authorization (C1)

**File:** `hypervisor/src/api/mod.rs`

Add admin user check. Simplest approach — environment allowlist:

```rust
// In hypervisor/src/main.rs or config
let admin_ids: HashSet<String> = std::env::var("CHOIR_ADMIN_USER_IDS")
    .unwrap_or_default()
    .split(',')
    .map(|s| s.trim().to_string())
    .collect();
```

In admin route handlers, check:
```rust
if !state.admin_ids.contains(&session.user_id) {
    return (StatusCode::FORBIDDEN, "not authorized").into_response();
}
```

**Files to modify:**
- `hypervisor/src/main.rs` — parse CHOIR_ADMIN_USER_IDS, add to AppState
- `hypervisor/src/api/mod.rs` — add check to admin handlers
- `nix/hosts/ovh-node.nix` — add env var to hypervisor service

### 1b. Secure Cookie Flag (H1)

**File:** `hypervisor/src/main.rs`

```rust
// Change line ~57
.with_secure(std::env::var("CHOIR_COOKIE_SECURE").as_deref() == Ok("true"))
```

Add to hypervisor service environment:
```nix
Environment = [ "CHOIR_COOKIE_SECURE=true" ];
```

### 1c. Sandbox Non-Root User (H3)

**File:** `nix/ch/sandbox-vm.nix`

```nix
users.users.choiros = {
  isSystemUser = true;
  group = "choiros";
  home = "/opt/choiros/data/sandbox";
};
users.groups.choiros = {};

systemd.services.choir-sandbox.serviceConfig = {
  User = "choiros";
  Group = "choiros";
  # Ensure data dir is writable
  ExecStartPre = "+${pkgs.coreutils}/bin/chown -R choiros:choiros /opt/choiros/data/sandbox";
};

# Change env file ownership so choiros user can read it
systemd.services.choir-extract-cmdline-secrets.script = ''
  set -euo pipefail
  ENV_FILE="/run/choiros-sandbox.env"
  : > "$ENV_FILE"
  for param in $(cat /proc/cmdline); do
    case "$param" in
      choir.gateway_token=*)
        echo "CHOIR_PROVIDER_GATEWAY_TOKEN=''${param#choir.gateway_token=}" >> "$ENV_FILE"
        ;;
    esac
  done
  chown choiros:choiros "$ENV_FILE"
  chmod 0600 "$ENV_FILE"
'';
```

### 1d. Hypervisor Hardening (H2)

**File:** `nix/hosts/ovh-node.nix`

Add to hypervisor service:
```nix
serviceConfig = {
  # ... existing config ...
  NoNewPrivileges = true;
  ProtectHome = true;
  PrivateTmp = true;
};
```

Full `ProtectSystem = "strict"` requires `ReadWritePaths` enumeration —
defer to Phase 3.

### 1e. Gateway Token File Permissions (H5)

**File:** `hypervisor/src/sandbox/systemd.rs`

After writing `gateway-token` file, set permissions:
```rust
std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))?;
```

---

## Phase 2: Per-VM Gateway Tokens (C3)

### 2a. HMAC Token Generation

**File:** `hypervisor/src/sandbox/mod.rs`

Replace single shared token with per-VM HMAC:

```rust
use hmac::{Hmac, Mac};
use sha2::Sha256;

fn generate_vm_token(master_key: &[u8], user_id: &str, sandbox_id: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(master_key)
        .expect("HMAC key length");
    mac.update(format!("{user_id}:{sandbox_id}").as_bytes());
    hex::encode(mac.finalize().into_bytes())
}
```

The `master_key` is the existing `provider_gateway_token` from platform
credentials.

### 2b. Gateway Token Verification

**File:** `hypervisor/src/provider_gateway.rs`

Replace bearer token string comparison with HMAC verification:

```rust
fn verify_vm_token(master_key: &[u8], token: &str, user_id: &str, sandbox_id: &str) -> bool {
    let expected = generate_vm_token(master_key, user_id, sandbox_id);
    // Constant-time comparison
    use subtle::ConstantTimeEq;
    token.as_bytes().ct_eq(expected.as_bytes()).into()
}
```

Extract `user_id` and `sandbox_id` from the token by trying all
registered sandboxes, or embed them as a prefix:
`token = "{user_id}:{sandbox_id}:{hmac}"`.

### 2c. Remove Self-Reported Headers (M5)

After 2b, the gateway derives user identity from the verified token.
Remove trust in `x-choiros-sandbox-id` and `x-choiros-user-id` headers.

**Dependencies:** `hmac`, `sha2`, `hex`, `subtle` crates.

---

## Phase 3: Network Isolation (H4)

### 3a. Replace trustedInterfaces with nftables

**File:** `nix/hosts/ovh-node.nix`

Remove:
```nix
networking.firewall.trustedInterfaces = [ "br-choiros" ];
```

Add:
```nix
networking.nftables.enable = true;
networking.nftables.ruleset = ''
  table inet choiros-isolation {
    chain forward {
      type filter hook forward priority 0; policy drop;
      iifname "br-choiros" oifname != "br-choiros" accept
      oifname "br-choiros" ct state established,related accept
    }
    chain input {
      type filter hook input priority 0;
      iifname "br-choiros" tcp dport 9090 accept
      iifname "br-choiros" udp dport { 67, 68 } accept
      iifname "br-choiros" drop
    }
  }
'';
```

### 3b. Verify Isolation

After deploy, from a running VM:
```bash
# Should succeed (gateway)
curl http://10.0.0.1:9090/health

# Should fail (other VM)
curl --max-time 3 http://10.0.0.103:8080/health

# Should fail (host SSH)
curl --max-time 3 http://10.0.0.1:22
```

---

## Phase 4: Hypervisor Bind Address (M4)

**File:** `hypervisor/src/main.rs`

Change bind address to 127.0.0.1:
```rust
let addr = std::env::var("CHOIR_BIND_ADDR")
    .unwrap_or_else(|_| "127.0.0.1:9090".to_string());
```

VMs access the gateway via the bridge IP (10.0.0.1), which is routed
through the kernel to 127.0.0.1 — but this requires NAT or a second
listener. Simpler: bind to both:
- `127.0.0.1:9090` for Caddy
- `10.0.0.1:9090` for VMs (bridge only)

Or keep `0.0.0.0:9090` and rely on the host firewall (already
configured to only allow 80/443/22 from internet).

---

## Verification Checklist

After each phase, verify:

- [ ] Admin endpoints return 403 for non-admin users
- [ ] Session cookie has Secure flag (check in browser devtools)
- [ ] Sandbox process runs as `choiros` user (check `ps aux` in guest)
- [ ] Gateway token file is mode 0600
- [ ] Per-VM tokens: one VM's token doesn't work for another VM's API calls
- [ ] VMs cannot ping each other on 10.0.0.0/24
- [ ] VMs cannot reach host ports other than 9090

---

## Files Summary

| Phase | File | Change |
|-------|------|--------|
| 1a | hypervisor/src/api/mod.rs | Admin auth check |
| 1a | hypervisor/src/main.rs | Parse CHOIR_ADMIN_USER_IDS |
| 1b | hypervisor/src/main.rs | Secure cookie flag |
| 1c | nix/ch/sandbox-vm.nix | choiros user + service User= |
| 1d | nix/hosts/ovh-node.nix | NoNewPrivileges, ProtectHome |
| 1e | hypervisor/src/sandbox/systemd.rs | Token file permissions |
| 2a | hypervisor/src/sandbox/mod.rs | HMAC token generation |
| 2b | hypervisor/src/provider_gateway.rs | HMAC verification |
| 3a | nix/hosts/ovh-node.nix | nftables rules |
| 4 | hypervisor/src/main.rs | Bind address |
