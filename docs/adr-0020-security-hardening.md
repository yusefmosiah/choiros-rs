# ADR-0020: Security Hardening — Multi-Tenant Isolation

Date: 2026-03-10
Kind: Decision
Status: Accepted
Priority: 1
Requires: [ADR-0014, ADR-0018]
Authors: wiz + Claude

## Narrative Summary (1-minute read)

A security architecture review of the VM isolation and multi-tenant
boundaries identified 3 critical, 5 high, and 4 medium findings. The
most severe: admin endpoints have zero authorization (any user can
control any VM), the gateway token is world-readable via /proc/cmdline,
and all VMs share a single gateway token with no per-user scoping.

This ADR defines the decisions to address each finding. No changes to
the VM hypervisor boundary itself — cloud-hypervisor hardware
virtualization is sound. All findings are in the application and
configuration layers.

## What Changed

- 2026-03-10: Initial security architecture review and ADR.

## What To Do Next

Implement fixes in priority order. See implementation guide:
`docs/adr-0020-implementation.md`

---

## Finding C1: Admin Endpoints Have No Authorization

### Context

`/admin/sandboxes/*` endpoints (start, stop, hibernate, swap, list)
use `require_auth` middleware which only checks that a session exists.
Any authenticated user can list all sandboxes and control any user's VM.

### Decision

Add role-based authorization to admin endpoints. Two approaches
(choose one):

**Option A (simple):** Admin allowlist by user ID in environment config.
Check `user_id in ADMIN_USER_IDS` in admin route handlers.

**Option B (proper):** Add `role` column to users table. Middleware
`require_admin` checks `session.role == "admin"`. Admin role assigned
via CLI or database migration.

Option A is sufficient for current scale (2 operators).

### Consequences

- Admin operations restricted to authorized users
- Existing sessions continue to work for non-admin routes

---

## Finding C2: Gateway Token Readable via /proc/cmdline

### Context

`choir.gateway_token=<TOKEN>` is passed on the kernel command line.
Inside Linux guests, `/proc/cmdline` is world-readable (mode 0444).
Any process — including user code executed by the sandbox agent's bash
tool — can read the token.

### Decision

Replace kernel cmdline injection with virtio-vsock or virtio-serial
secret delivery:

**Phase 1 (immediate mitigation):** The `choir-extract-cmdline-secrets`
oneshot already writes the token to `/run/choiros-sandbox.env` (mode
0600). Add a second step that overwrites the token in `/proc/cmdline`
by writing to `/sys/module/kernel/parameters/` — but this is read-only
on most kernels.

**Phase 1 (actual):** Run the sandbox process as a non-root user
(fixes H3 too). A non-root sandbox cannot read files owned by root with
mode 0600. The /proc/cmdline exposure remains but is mitigated by:
- Sandbox binary itself doesn't need the token (it's in the env file)
- Agent bash tool runs as the sandbox user, not root
- Defense in depth: the token only grants gateway access, not direct
  API key access

**Phase 2 (proper fix):** Switch to virtio-vsock for secret delivery.
cloud-hypervisor supports vsock. The host hypervisor connects to the
guest vsock, sends the token, and the guest oneshot receives it. No
kernel cmdline, no /proc exposure, no disk persistence.

### Consequences

- Phase 1: Reduced exposure (non-root sandbox can't read env file)
- Phase 2: Complete elimination of cmdline secret exposure

---

## Finding C3: Single Shared Gateway Token

### Context

All VMs receive the same `provider_gateway_token`. The `x-choiros-sandbox-id`
and `x-choiros-user-id` headers used for rate limiting are self-reported
by the sandbox and unverified by the gateway.

### Decision

**Phase 1:** Generate per-VM HMAC tokens. The hypervisor generates
`HMAC-SHA256(master_key, user_id + sandbox_id)` and injects it as the
gateway token. The gateway verifies the HMAC and extracts the user_id,
ignoring self-reported headers.

**Phase 2:** Short-lived JWT tokens with expiry. The hypervisor issues
JWTs containing `user_id`, `sandbox_id`, `exp`. The gateway validates
the signature and rejects expired tokens.

Phase 1 is sufficient — the master key never leaves the hypervisor
process, and HMAC verification is fast (no database lookup).

### Consequences

- Per-user token binding: a compromised sandbox token only grants
  access as that specific user
- Self-reported headers replaced by cryptographically verified identity
- Rate limiting becomes per-user (derived from verified token)

---

## Finding H1: Session Cookie Not Marked Secure

### Decision

Set `with_secure(true)` in production. Use environment variable to
control: `CHOIR_COOKIE_SECURE=true` (default in production).

---

## Finding H2: Hypervisor Runs as Root

### Decision

Add systemd hardening to the hypervisor service unit:

```nix
NoNewPrivileges = true;
ProtectSystem = "strict";
ProtectHome = true;
ReadWritePaths = [ "/opt/choiros" "/var/lib/dnsmasq" ];
CapabilityBoundingSet = [ "CAP_NET_ADMIN" ];  # for bridge/tap management
AmbientCapabilities = [ "CAP_NET_ADMIN" ];
```

The hypervisor needs root-equivalent access for `systemctl start/stop`
of cloud-hypervisor@ units. Options:
- **polkit rule** allowing the choiros user to manage `cloud-hypervisor@*`
- **sudoers entry** for specific systemctl commands
- **socket activation** where systemd manages the lifecycle

For now, keep running as root but add `NoNewPrivileges` and filesystem
restrictions to limit blast radius.

---

## Finding H3: Sandbox Runs as Root Inside Guest

### Decision

Add a dedicated `choiros` user in the guest VM and run the sandbox
service as that user:

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
};
```

The sandbox process needs write access to its data directory
(`/opt/choiros/data/sandbox/`) but not root privileges. The agent's
bash tool will run as `choiros`, limiting filesystem access.

### Consequences

- Sandbox cannot read `/run/choiros-sandbox.env` if owned by root
  (mitigates C2) — change env file ownership to `choiros:choiros`
- Agent bash tool confined to choiros user permissions
- Cannot modify system files or install packages (defense in depth)

---

## Finding H4: No Inter-VM Network Isolation

### Context

`trustedInterfaces = [ "br-choiros" ]` disables the firewall on the
bridge. VMs can reach each other directly on 10.0.0.0/24, access the
host's services, and potentially ARP-spoof.

### Decision

Replace `trustedInterfaces` with explicit nftables rules:

1. **VM→host:** Allow only DHCP (UDP 67/68) and provider gateway
   (TCP 9090 on 10.0.0.1). Block all other host ports.
2. **VM→VM:** Block all inter-VM traffic on br-choiros. Each VM
   should only communicate with the host gateway.
3. **VM→internet:** Allow outbound via NAT (already configured).
4. **ARP isolation:** Enable `ebtables` or nftables bridge filtering
   to prevent ARP spoofing.

```nix
networking.nftables.enable = true;
networking.nftables.ruleset = ''
  table inet choiros-isolation {
    chain forward {
      type filter hook forward priority 0; policy drop;
      # Allow VM→internet (via NAT)
      iifname "br-choiros" oifname != "br-choiros" accept
      # Allow return traffic
      oifname "br-choiros" ct state established,related accept
      # Block VM→VM (implicit drop)
    }
    chain input {
      type filter hook input priority 0;
      iifname "br-choiros" tcp dport 9090 accept  # gateway
      iifname "br-choiros" udp dport { 67, 68 } accept  # DHCP
      iifname "br-choiros" drop  # block everything else from VMs
    }
  }
'';
```

### Consequences

- VMs fully isolated from each other
- VMs can only reach the provider gateway on the host
- ARP spoofing prevented
- Removes `trustedInterfaces` (which was a blanket firewall bypass)

---

## Finding H5: Gateway Token Written to Disk

### Decision

**Phase 1:** Restrict file permissions on `gateway-token` to mode 0600,
owned by root. Clean up the file after the VM reads it (delete after
successful boot confirmation).

**Phase 2:** Eliminated entirely by C2 Phase 2 (virtio-vsock delivery).

---

## Findings M1-M5: Medium Priority

| ID | Finding | Fix |
|----|---------|-----|
| M1 | Debug dumps to /tmp | Gate behind explicit env flag, use /run/choiros/ with 0600 |
| M2 | KSM side-channel | Accept risk (not hosting adversarial tenants) |
| M3 | Seccomp status unclear | Verify `--seccomp true` in cloud-hypervisor@ cmdline |
| M4 | Hypervisor on 0.0.0.0 | Bind to 127.0.0.1, let Caddy handle external traffic |
| M5 | Self-reported gateway headers | Fixed by C3 (HMAC tokens replace self-reported headers) |

---

## Positive Design Decisions (Already Sound)

- Hardware VM isolation via cloud-hypervisor (not containers)
- Zero API keys in sandbox — all LLM calls route through gateway
- Cookie/auth headers stripped before proxying to sandbox
- Per-user btrfs subvolumes with CoW snapshots
- Provider gateway upstream allowlist (anti-SSRF)
- NAT topology prevents inbound internet→VM traffic

---

## Sources

- [Linux Kernel Self-Protection](https://www.kernel.org/doc/html/latest/security/self-protection.html)
- [CVE-2026-24834: Kata virtio-pmem+DAX escape](https://edera.dev/stories/cve-2026-24834-when-trusting-the-guest-goes-wrong)
- [KSM Timing Side-Channels (academic)](https://www.usenix.org/conference/usenixsecurity14/technical-sessions/presentation/barresi)
- [cloud-hypervisor seccomp documentation](https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/docs/security.md)
