# ADR-0017 Implementation Guide: systemd-Native VM Lifecycle

## Narrative Summary (1-minute read)

Replace `scripts/ops/ovh-runtime-ctl.sh` with systemd unit templates for process lifecycle
and a Rust `SystemdLifecycle` module for policy logic. The hypervisor calls `systemctl`
instead of shelling out to bash. systemd handles orphan cleanup (KillMode=control-group),
restart-on-failure, dependency ordering, and journald logging.

## Phase 1: systemd Unit Templates (nix config)

Add 4 unit templates to `nix/hosts/ovh-node.nix`. Instance ID = `{user_short}-{role}`
(e.g., `default-live`).

### 1.1 tap-setup@.service (oneshot)

Creates TAP device, adds to bridge, assigns MAC. Teardown on stop.

```nix
systemd.services."tap-setup@" = {
  description = "TAP device for sandbox %i";
  serviceConfig = {
    Type = "oneshot";
    RemainAfterExit = true;
    ExecStart = writeScript "tap-setup" { ... };
    ExecStop = writeScript "tap-teardown" { ... };
  };
};
```

Key logic:
- Derive TAP name from instance: `tap-%i` (truncated to 15 chars for IFNAMSIZ)
- Derive MAC from instance ID hash (deterministic)
- `ip tuntap add`, `ip link set master br-choiros`, `ip link set up`
- ExecStop: `ip link del tap-%i`

### 1.2 virtiofsd@.service

Two virtiofs shares per VM (nix-store read-only, credentials read-only).

```nix
systemd.services."virtiofsd-nix@" = {
  description = "virtiofsd nix-store for sandbox %i";
  requires = [ "tap-setup@%i.service" ];
  after = [ "tap-setup@%i.service" ];
  serviceConfig = {
    Type = "simple";
    ExecStart = "${pkgs.virtiofsd}/bin/virtiofsd --socket-path=/opt/choiros/vms/state/%i/virtiofsd-nix.sock --shared-dir /nix/store --sandbox none";
    KillMode = "control-group";
  };
};

systemd.services."virtiofsd-creds@" = {
  description = "virtiofsd credentials for sandbox %i";
  requires = [ "tap-setup@%i.service" ];
  after = [ "tap-setup@%i.service" ];
  serviceConfig = {
    Type = "simple";
    ExecStart = "${pkgs.virtiofsd}/bin/virtiofsd --socket-path=/opt/choiros/vms/state/%i/virtiofsd-creds.sock --shared-dir /run/choiros/credentials/sandbox --sandbox none";
    KillMode = "control-group";
  };
};
```

### 1.3 cloud-hypervisor@.service

VM process. Depends on virtiofsd and TAP.

```nix
systemd.services."cloud-hypervisor@" = {
  description = "Cloud Hypervisor VM for sandbox %i";
  requires = [ "virtiofsd-nix@%i.service" "virtiofsd-creds@%i.service" ];
  after = [ "virtiofsd-nix@%i.service" "virtiofsd-creds@%i.service" ];
  serviceConfig = {
    Type = "simple";
    # ExecStart is generated dynamically by hypervisor writing a config file
    # before starting the unit. The script reads config from state dir.
    ExecStart = writeScript "ch-start" { ... };
    KillMode = "control-group";
    TimeoutStopSec = 15;
    # Do NOT set Restart — hypervisor manages lifecycle decisions
  };
};
```

**Important:** cloud-hypervisor has two start modes:
1. **Cold boot:** `microvm-run` script (from vmRunnerLive)
2. **Snapshot restore:** `cloud-hypervisor --restore source_url=file://... --api-socket ...`

The start script reads a mode file from state dir:
- `/opt/choiros/vms/state/%i/boot-mode` = `cold` or `restore`
- For cold boot: exec microvm-run
- For restore: exec cloud-hypervisor --restore

### 1.4 socat-sandbox@.service

Port forwarding from localhost to VM IP.

```nix
systemd.services."socat-sandbox@" = {
  description = "Socat port forward for sandbox %i";
  requires = [ "cloud-hypervisor@%i.service" ];
  after = [ "cloud-hypervisor@%i.service" ];
  bindsTo = [ "cloud-hypervisor@%i.service" ];
  serviceConfig = {
    Type = "simple";
    # Reads port and VM IP from state dir config
    ExecStart = writeScript "socat-fwd" { ... };
    KillMode = "control-group";
  };
};
```

`BindsTo` ensures socat dies when cloud-hypervisor stops.

## Phase 2: Rust SystemdLifecycle Module

New file: `hypervisor/src/sandbox/systemd.rs`

### 2.1 Core struct

```rust
pub struct SystemdLifecycle {
    state_base: PathBuf,  // /opt/choiros/vms/state
    vm_runner_dir: PathBuf,
}
```

### 2.2 Key methods

```rust
impl SystemdLifecycle {
    /// Prepare state dir, write boot-mode file, start systemd chain.
    pub async fn ensure(&self, instance: &str, config: &VmConfig) -> Result<()> {
        self.prepare_state_dir(instance, config).await?;
        self.write_boot_mode(instance).await?;
        systemctl_start(&format!("socat-sandbox@{instance}")).await?;
        // systemd pulls in the full dependency chain automatically
        Ok(())
    }

    /// Stop socat → VM → virtiofsd → TAP (reverse dependency order).
    pub async fn stop(&self, instance: &str) -> Result<()> {
        systemctl_stop(&format!("socat-sandbox@{instance}")).await?;
        systemctl_stop(&format!("cloud-hypervisor@{instance}")).await?;
        systemctl_stop(&format!("virtiofsd-nix@{instance}")).await?;
        systemctl_stop(&format!("virtiofsd-creds@{instance}")).await?;
        systemctl_stop(&format!("tap-setup@{instance}")).await?;
        Ok(())
    }

    /// Hibernate: pause + snapshot via CH API, then stop units.
    pub async fn hibernate(&self, instance: &str) -> Result<()> {
        let api_sock = self.state_base.join(instance).join("api.sock");
        ch_api_pause(&api_sock).await?;
        ch_api_snapshot(&api_sock, &self.snapshot_dir(instance)).await?;
        self.stop(instance).await?;
        Ok(())
    }

    /// Check if instance is running.
    pub async fn is_active(&self, instance: &str) -> bool {
        systemctl_is_active(&format!("cloud-hypervisor@{instance}")).await
    }
}
```

### 2.3 systemctl helpers

```rust
async fn systemctl_start(unit: &str) -> Result<()> {
    let status = Command::new("systemctl")
        .args(["start", unit])
        .status().await?;
    if !status.success() {
        anyhow::bail!("systemctl start {unit} failed: {:?}", status.code());
    }
    Ok(())
}

async fn systemctl_stop(unit: &str) -> Result<()> {
    let status = Command::new("systemctl")
        .args(["stop", unit])
        .status().await?;
    // Stop is best-effort — unit may already be stopped
    Ok(())
}

async fn systemctl_is_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status().await
        .map(|s| s.success())
        .unwrap_or(false)
}
```

### 2.4 Cloud-hypervisor API helpers

```rust
async fn ch_api_pause(api_sock: &Path) -> Result<()> {
    // PUT http://localhost/api/v1/vm.pause via unix socket
    let client = unix_socket_client(api_sock);
    client.put("/api/v1/vm.pause").send().await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok(())
}

async fn ch_api_snapshot(api_sock: &Path, dest: &Path) -> Result<()> {
    let body = serde_json::json!({
        "destination_url": format!("file://{}", dest.display())
    });
    let client = unix_socket_client(api_sock);
    let resp = client.put("/api/v1/vm.snapshot")
        .json(&body).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("snapshot failed: {}", resp.text().await?);
    }
    Ok(())
}

async fn ch_api_resume(api_sock: &Path) -> Result<()> {
    let client = unix_socket_client(api_sock);
    client.put("/api/v1/vm.resume").send().await?;
    Ok(())
}
```

### 2.5 btrfs policy (stays in Rust)

```rust
impl SystemdLifecycle {
    async fn ensure_user_subvolume(&self, user_id: &str) -> Result<PathBuf> {
        let user_dir = PathBuf::from("/data/users").join(user_id);
        if !user_dir.exists() {
            Command::new("btrfs")
                .args(["subvolume", "create", &user_dir.to_string_lossy()])
                .status().await?;
        }
        Ok(user_dir)
    }

    async fn snapshot_data(&self, user_id: &str, label: &str) -> Result<()> {
        let src = PathBuf::from("/data/users").join(user_id);
        let dst = PathBuf::from("/data/snapshots")
            .join(format!("{user_id}-{label}-{}", chrono::Utc::now().timestamp()));
        Command::new("btrfs")
            .args(["subvolume", "snapshot", "-r",
                   &src.to_string_lossy(), &dst.to_string_lossy()])
            .status().await?;
        Ok(())
    }
}
```

## Phase 3: Wire into SandboxRegistry

Replace `run_runtime_ctl()` calls with `SystemdLifecycle` calls:

1. `SandboxRegistry::new()` takes `SystemdLifecycle` instead of `runtime_ctl: String`
2. `spawn_runtime()` calls `lifecycle.ensure(instance, config)`
3. `stop_handle()` calls `lifecycle.stop(instance)`
4. `hibernate_handle()` calls `lifecycle.hibernate(instance)`
5. Remove `SandboxHandle::RuntimeCtl` variant — instance ID is sufficient

## Phase 4: E2E Verification

### Happy path tests (Playwright)

1. **Auth + desktop load** — register, wait for desktop, verify render
2. **Sandbox restart after stop** — admin stop, then ensure re-starts VM
3. **Concurrent registrations** — 5 users register, all get working sandboxes
4. **Idle watchdog hibernate** — set short timeout, verify hibernate, verify restore

### Error path tests (integration, on Node B)

1. **Kill cloud-hypervisor mid-run** — verify systemd restarts, or hypervisor detects failure
2. **Kill virtiofsd** — verify cloud-hypervisor unit stops (BindsTo dependency)
3. **Double ensure** — verify idempotent (no duplicate processes)
4. **Orphan audit** — after test suite, verify no orphan virtiofsd/cloud-hypervisor/socat

### Orphan audit script

```bash
#!/bin/bash
# Run after E2E suite — should find zero orphans
echo "=== Orphan Process Audit ==="
echo "virtiofsd: $(pgrep -c virtiofsd 2>/dev/null || echo 0)"
echo "cloud-hypervisor: $(pgrep -c cloud-hypervisor 2>/dev/null || echo 0)"
echo "socat: $(pgrep -c -f 'socat.*TCP-LISTEN' 2>/dev/null || echo 0)"
echo ""
echo "=== systemd sandbox units ==="
systemctl list-units 'cloud-hypervisor@*' 'virtiofsd-*@*' 'socat-sandbox@*' 'tap-setup@*' --no-legend
```

## Migration Checklist

- [ ] Add systemd unit templates to `nix/hosts/ovh-node.nix`
- [ ] Add state dir structure: `/opt/choiros/vms/state/{instance}/`
- [ ] Create `hypervisor/src/sandbox/systemd.rs`
- [ ] Add hyper-util or reqwest unix socket support to hypervisor Cargo.toml
- [ ] Wire `SystemdLifecycle` into `SandboxRegistry`
- [ ] Update hypervisor env vars (remove SANDBOX_VFKIT_CTL, add state paths)
- [ ] Deploy to Node B (`nixos-rebuild switch`)
- [ ] Run E2E suite against Node B
- [ ] Run orphan audit
- [ ] Remove `scripts/ops/ovh-runtime-ctl.sh`
- [ ] Remove runtime-ctl nix derivation from `flake.nix`
