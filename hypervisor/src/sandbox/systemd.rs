//! systemd-native VM lifecycle management (ADR-0017).
//!
//! Replaces the bash runtime-ctl script with systemd unit templates for process
//! supervision and Rust for policy logic (btrfs, boot mode selection, etc.).

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;
use tracing::{error, info, warn};

/// systemd-based VM lifecycle manager.
///
/// Each VM instance is identified by a string like `live` or `dev`.
/// systemd unit templates are parameterized by this instance ID:
///   - `tap-setup@{instance}.service`
///   - `virtiofsd@{instance}.service`
///   - `cloud-hypervisor@{instance}.service`
///   - `socat-sandbox@{instance}.service`
#[allow(dead_code)] // public API — new() and is_active() used after full rollout
pub struct SystemdLifecycle {
    /// Base directory for VM state: /opt/choiros/vms/state
    state_base: PathBuf,
    /// Base directory for per-user btrfs subvolumes: /data/users
    user_data_dir: PathBuf,
    /// Base directory for btrfs snapshots: /data/snapshots
    snapshot_dir: PathBuf,
    /// Path to the microvm runner directory (contains bin/microvm-run, bin/virtiofsd-run)
    vm_runner_dir: PathBuf,
}

impl SystemdLifecycle {
    #[allow(dead_code)]
    pub fn new(
        state_base: PathBuf,
        user_data_dir: PathBuf,
        snapshot_dir: PathBuf,
        vm_runner_dir: PathBuf,
    ) -> Self {
        Self {
            state_base,
            user_data_dir,
            snapshot_dir,
            vm_runner_dir,
        }
    }

    /// Build from environment variables (as currently configured in systemd unit).
    /// Requires `CHOIR_SYSTEMD_LIFECYCLE=1` to opt in (ADR-0017 rollout control).
    pub fn from_env() -> Option<Self> {
        if std::env::var("CHOIR_SYSTEMD_LIFECYCLE").unwrap_or_default() != "1" {
            return None;
        }
        let vm_runner_dir = std::env::var("CHOIR_VM_RUNNER_DIR").ok()?;
        Some(Self {
            state_base: PathBuf::from(
                std::env::var("CHOIR_VM_STATE_DIR")
                    .unwrap_or_else(|_| "/opt/choiros/vms/state".to_string()),
            ),
            user_data_dir: PathBuf::from(
                std::env::var("CHOIR_USER_DATA_DIR").unwrap_or_else(|_| "/data/users".to_string()),
            ),
            snapshot_dir: PathBuf::from(
                std::env::var("CHOIR_SNAPSHOT_DIR")
                    .unwrap_or_else(|_| "/data/snapshots".to_string()),
            ),
            vm_runner_dir: PathBuf::from(vm_runner_dir),
        })
    }

    fn instance_state_dir(&self, instance: &str) -> PathBuf {
        self.state_base.join(instance)
    }

    fn api_sock_path(&self, instance: &str) -> PathBuf {
        self.instance_state_dir(instance)
            .join(format!("sandbox-{instance}.sock"))
    }

    fn vm_snapshot_dir(&self, instance: &str) -> PathBuf {
        self.instance_state_dir(instance).join("vm-snapshot")
    }

    fn boot_mode_path(&self, instance: &str) -> PathBuf {
        self.instance_state_dir(instance).join("boot-mode")
    }

    // ── btrfs policy ──────────────────────────────────────────────────────

    /// Create per-user btrfs subvolume if it doesn't exist.
    pub async fn ensure_user_subvolume(&self, user_id: &str) -> anyhow::Result<PathBuf> {
        let user_dir = self.user_data_dir.join(user_id);
        if user_dir.exists() {
            return Ok(user_dir);
        }
        info!(user_id, path = %user_dir.display(), "creating btrfs subvolume");
        let status = Command::new("btrfs")
            .args(["subvolume", "create"])
            .arg(&user_dir)
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!(
                "btrfs subvolume create failed for {}: {:?}",
                user_dir.display(),
                status.code()
            );
        }
        Ok(user_dir)
    }

    /// Snapshot user data via btrfs (instant CoW snapshot). Prunes old snapshots.
    pub async fn snapshot_data(&self, user_id: &str, label: &str) -> anyhow::Result<()> {
        let user_dir = self.user_data_dir.join(user_id);
        if !user_dir.exists() {
            info!(user_id, "no user subvolume, skipping data snapshot");
            return Ok(());
        }

        let snap_dir = self.snapshot_dir.join(user_id);
        tokio::fs::create_dir_all(&snap_dir).await?;

        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let snap_name = format!("{timestamp}-{label}");
        let snap_path = snap_dir.join(&snap_name);

        info!(user_id, snapshot = %snap_path.display(), "creating data snapshot");
        let status = Command::new("btrfs")
            .args(["subvolume", "snapshot", "-r"])
            .arg(&user_dir)
            .arg(&snap_path)
            .status()
            .await?;
        if !status.success() {
            warn!(user_id, "btrfs snapshot failed: {:?}", status.code());
        }

        // Prune old snapshots (keep last 3 for this label pattern)
        self.prune_snapshots(user_id, label, 3).await;

        Ok(())
    }

    async fn prune_snapshots(&self, user_id: &str, label: &str, keep: usize) {
        let snap_dir = self.snapshot_dir.join(user_id);
        let Ok(mut entries) = tokio::fs::read_dir(&snap_dir).await else {
            return;
        };

        let mut matching: Vec<String> = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(&format!("-{label}")) {
                matching.push(name);
            }
        }
        matching.sort();

        if matching.len() <= keep {
            return;
        }

        let to_remove = matching.len() - keep;
        for name in matching.into_iter().take(to_remove) {
            let path = snap_dir.join(&name);
            info!(user_id, snapshot = %path.display(), "pruning old snapshot");
            let _ = Command::new("btrfs")
                .args(["subvolume", "delete"])
                .arg(&path)
                .status()
                .await;
        }
    }

    /// Symlink per-user data.img from btrfs subvolume into VM state dir.
    pub async fn link_user_data_img(&self, user_id: &str, instance: &str) -> anyhow::Result<()> {
        let user_dir = self.ensure_user_subvolume(user_id).await?;
        let target = user_dir.join("data.img");
        let link = self.instance_state_dir(instance).join("data.img");

        // Check if already correctly linked
        if let Ok(existing) = tokio::fs::read_link(&link).await {
            if existing == target {
                // Ensure the target file actually exists
                if target.exists() {
                    return Ok(());
                }
                // Dangling symlink — fall through to create data.img
            }
        }

        // Handle migration: if a real file exists at link path, move it
        if link.exists() && !link.is_symlink() {
            if !target.exists() {
                info!(user_id, "migrating existing data.img to btrfs subvolume");
                // Use cp + rm instead of rename — files may be on different filesystems
                let status = Command::new("cp")
                    .args(["--reflink=auto", "--sparse=always"])
                    .arg(&link)
                    .arg(&target)
                    .status()
                    .await?;
                if status.success() {
                    tokio::fs::remove_file(&link).await?;
                } else {
                    warn!(user_id, "cp data.img failed, will create fresh");
                }
            } else {
                warn!(
                    user_id,
                    "data.img exists both locally and on btrfs, removing local copy"
                );
                tokio::fs::remove_file(&link).await?;
            }
        }

        // Create data.img if it doesn't exist (first boot for this user)
        if !target.exists() {
            info!(user_id, path = %target.display(), "creating data.img (2GB)");
            let status = Command::new("truncate")
                .args(["-s", "2G"])
                .arg(&target)
                .status()
                .await?;
            if !status.success() {
                anyhow::bail!("truncate data.img failed: {:?}", status.code());
            }
            let status = Command::new("mkfs.ext4")
                .args(["-q", "-F"])
                .arg(&target)
                .status()
                .await?;
            if !status.success() {
                anyhow::bail!("mkfs.ext4 data.img failed: {:?}", status.code());
            }
            info!(user_id, "data.img created and formatted");
        }

        // Remove stale symlink if pointing elsewhere
        let _ = tokio::fs::remove_file(&link).await;

        tokio::fs::symlink(&target, &link).await?;
        info!(
            user_id,
            link = %link.display(),
            target = %target.display(),
            "data.img symlinked"
        );
        Ok(())
    }

    // ── systemd lifecycle ─────────────────────────────────────────────────

    /// Prepare state dir and start the full VM chain via systemd.
    ///
    /// This is the replacement for `run_runtime_ctl("ensure", ...)`.
    pub async fn ensure(&self, instance: &str, user_id: &str, port: u16) -> anyhow::Result<()> {
        let state_dir = self.instance_state_dir(instance);
        tokio::fs::create_dir_all(&state_dir).await?;

        // btrfs setup (policy logic stays in Rust)
        self.link_user_data_img(user_id, instance).await?;

        // Determine boot mode
        let has_snapshot = self.vm_snapshot_dir(instance).join("state.json").exists();
        let boot_mode = if has_snapshot { "restore" } else { "cold" };
        tokio::fs::write(self.boot_mode_path(instance), boot_mode).await?;
        info!(instance, boot_mode, "prepared VM state dir");

        // If snapshot restore, clean stale virtiofsd sockets so fresh ones are created
        if has_snapshot {
            self.clean_stale_sockets(instance).await;
        }

        // Start the full chain. systemd handles dependency ordering:
        // socat-sandbox@ requires cloud-hypervisor@ requires virtiofsd@ requires tap-setup@
        // Starting the leaf unit pulls in the entire chain.
        systemctl_start(&format!("socat-sandbox@{instance}")).await?;

        info!(instance, port, "VM chain started via systemd");
        Ok(())
    }

    /// Stop the full VM chain and optionally snapshot data.
    ///
    /// Replacement for `run_runtime_ctl("stop", ...)`.
    pub async fn stop(&self, instance: &str, user_id: &str) -> anyhow::Result<()> {
        // Snapshot user data before stopping
        self.snapshot_data(user_id, instance).await?;

        // Stop units in reverse dependency order.
        // BindsTo should cascade, but explicit stops are more reliable.
        systemctl_stop(&format!("socat-sandbox@{instance}")).await;
        systemctl_stop(&format!("cloud-hypervisor@{instance}")).await;
        systemctl_stop(&format!("virtiofsd@{instance}")).await;
        systemctl_stop(&format!("tap-setup@{instance}")).await;

        // Clean up VM snapshot since this is a hard stop
        let snap = self.vm_snapshot_dir(instance);
        if snap.exists() {
            let _ = tokio::fs::remove_dir_all(&snap).await;
        }

        info!(instance, "VM chain stopped");
        Ok(())
    }

    /// Hibernate: pause VM, snapshot VM state, stop process chain.
    /// Preserves VM snapshot for fast restore on next ensure.
    ///
    /// Replacement for `run_runtime_ctl("hibernate", ...)`.
    pub async fn hibernate(&self, instance: &str, user_id: &str) -> anyhow::Result<()> {
        let api_sock = self.api_sock_path(instance);

        // Snapshot user data
        self.snapshot_data(user_id, instance).await?;

        // Pause VM vCPUs
        if let Err(e) = ch_api_put(&api_sock, "vm.pause").await {
            warn!(instance, "VM pause failed: {e}, falling back to hard stop");
            return self.stop(instance, user_id).await;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Snapshot VM state to disk
        let snap_dir = self.vm_snapshot_dir(instance);
        tokio::fs::create_dir_all(&snap_dir).await?;

        if let Err(e) = ch_api_snapshot(&api_sock, &snap_dir).await {
            warn!(
                instance,
                "VM snapshot failed: {e}, resuming and falling back to hard stop"
            );
            let _ = ch_api_put(&api_sock, "vm.resume").await;
            return self.stop(instance, user_id).await;
        }

        info!(instance, snapshot = %snap_dir.display(), "VM state saved");

        // Stop process chain (state is on disk now)
        // Don't stop tap-setup — keep TAP alive for fast restore
        systemctl_stop(&format!("socat-sandbox@{instance}")).await;
        systemctl_stop(&format!("cloud-hypervisor@{instance}")).await;
        systemctl_stop(&format!("virtiofsd@{instance}")).await;

        info!(instance, "VM hibernated (TAP kept alive)");
        Ok(())
    }

    /// Check if the VM is currently running.
    #[allow(dead_code)]
    pub async fn is_active(&self, instance: &str) -> bool {
        systemctl_is_active(&format!("cloud-hypervisor@{instance}")).await
    }

    async fn clean_stale_sockets(&self, instance: &str) {
        let state_dir = self.instance_state_dir(instance);
        if let Ok(mut entries) = tokio::fs::read_dir(&state_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.contains("virtiofs") && name.ends_with(".sock") {
                    let _ = tokio::fs::remove_file(entry.path()).await;
                }
            }
        }
    }
}

// ── systemctl helpers ─────────────────────────────────────────────────────

async fn systemctl_start(unit: &str) -> anyhow::Result<()> {
    info!(unit, "systemctl start");
    let output = Command::new("systemctl")
        .args(["start", unit])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!(unit, stderr = %stderr, "systemctl start failed");
        anyhow::bail!("systemctl start {unit} failed: {stderr}");
    }
    Ok(())
}

async fn systemctl_stop(unit: &str) {
    info!(unit, "systemctl stop");
    let result = Command::new("systemctl")
        .args(["stop", unit])
        .output()
        .await;
    match result {
        Ok(output) if !output.status.success() => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(unit, stderr = %stderr, "systemctl stop returned non-zero (may be already stopped)");
        }
        Err(e) => {
            warn!(unit, error = %e, "systemctl stop failed to execute");
        }
        _ => {}
    }
}

#[allow(dead_code)]
async fn systemctl_is_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

// ── cloud-hypervisor API helpers ──────────────────────────────────────────

/// Send a PUT request to the cloud-hypervisor API via unix socket.
/// Uses curl for simplicity (same as the bash script).
async fn ch_api_put(api_sock: &Path, endpoint: &str) -> anyhow::Result<()> {
    let output = Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "10",
            "--unix-socket",
            &api_sock.to_string_lossy(),
            "-X",
            "PUT",
            &format!("http://localhost/api/v1/{endpoint}"),
        ])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("CH API {endpoint} failed: {stderr}");
    }
    let body = String::from_utf8_lossy(&output.stdout);
    if body.to_lowercase().contains("error") {
        anyhow::bail!("CH API {endpoint} returned error: {body}");
    }
    Ok(())
}

/// Snapshot VM state via cloud-hypervisor API.
async fn ch_api_snapshot(api_sock: &Path, dest: &Path) -> anyhow::Result<()> {
    let body = serde_json::json!({
        "destination_url": format!("file://{}", dest.display())
    });
    let output = Command::new("curl")
        .args([
            "-s",
            "--max-time",
            "30",
            "--unix-socket",
            &api_sock.to_string_lossy(),
            "-X",
            "PUT",
            &format!("http://localhost/api/v1/vm.snapshot"),
            "-H",
            "Content-Type: application/json",
            "-d",
            &body.to_string(),
        ])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("CH API vm.snapshot failed: {stderr}");
    }
    let response = String::from_utf8_lossy(&output.stdout);
    if response.to_lowercase().contains("error") {
        anyhow::bail!("CH API vm.snapshot returned error: {response}");
    }
    Ok(())
}
