pub mod systemd;

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use dashmap::{DashMap, DashSet};
use tokio::{net::TcpStream, process::Command, sync::watch, time::sleep};
use tracing::{error, info, warn};

use self::systemd::SystemdLifecycle;

// ── Memory pressure helpers (ADR-0018) ──────────────────────────────────────

/// Read MemAvailable from /proc/meminfo (in MB).
fn read_available_memory_mb() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

/// Read available memory as percentage of total.
fn read_memory_percent_available() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb = 0u64;
    let mut avail_kb = 0u64;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total_kb = rest.split_whitespace().next()?.parse().ok()?;
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = rest.split_whitespace().next()?.parse().ok()?;
        }
    }
    if total_kb == 0 {
        return None;
    }
    Some(avail_kb * 100 / total_kb)
}

/// Count running VMs across all users.
fn count_running_vms(entries: &DashMap<String, UserSandboxes>) -> usize {
    entries
        .iter()
        .map(|entry| {
            let u = entry.value();
            u.roles
                .values()
                .filter(|e| matches!(e.status, SandboxStatus::Running))
                .count()
                + u.branches
                    .values()
                    .filter(|e| matches!(e.status, SandboxStatus::Running))
                    .count()
        })
        .sum()
}
const MIN_AVAILABLE_MB: u64 = 1024; // 1 GB minimum before spawning new VM

fn validate_branch_name(branch: &str) -> anyhow::Result<()> {
    if branch.trim().is_empty() {
        return Err(anyhow::anyhow!("branch name cannot be empty"));
    }

    if branch.len() > 64 {
        return Err(anyhow::anyhow!(
            "branch name too long (max 64 chars): {branch}"
        ));
    }

    if !branch
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(anyhow::anyhow!(
            "invalid branch name '{branch}' (allowed: [A-Za-z0-9._-])"
        ));
    }

    Ok(())
}

/// The role of a sandbox instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SandboxRole {
    /// The live sandbox — receives all default proxied traffic.
    Live,
    /// The dev sandbox — only reachable via the `/dev/` path prefix.
    Dev,
}

impl std::fmt::Display for SandboxRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxRole::Live => write!(f, "live"),
            SandboxRole::Dev => write!(f, "dev"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SandboxStatus {
    Running,
    Stopped,
    /// VM snapshot saved to disk — fast restore available via `runtime_ctl ensure`.
    Hibernated,
    /// Boot in progress — concurrent requests join the watch channel (ADR-0022).
    Starting(watch::Receiver<Result<u16, String>>),
    /// Process exited unexpectedly.
    Failed,
}

impl PartialEq for SandboxStatus {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Running, Self::Running)
                | (Self::Stopped, Self::Stopped)
                | (Self::Hibernated, Self::Hibernated)
                | (Self::Starting(_), Self::Starting(_))
                | (Self::Failed, Self::Failed)
        )
    }
}

impl Eq for SandboxStatus {}

pub struct SandboxEntry {
    pub role: Option<SandboxRole>,
    pub branch: Option<String>,
    pub port: u16,
    pub status: SandboxStatus,
    pub last_activity: Instant,
    pub handle: Option<SandboxHandle>,
}

pub enum SandboxHandle {
    RuntimeCtl(RuntimeCtlHandle),
}

pub struct RuntimeCtlHandle {
    pub user_id: String,
    pub runtime_name: String,
    pub role: Option<SandboxRole>,
    pub branch: Option<String>,
    pub port: u16,
}

#[derive(Default)]
struct UserSandboxes {
    roles: HashMap<SandboxRole, SandboxEntry>,
    branches: HashMap<String, SandboxEntry>,
}

/// Atomic port allocator using DashSet (ADR-0022).
/// `DashSet::insert` returns false if already present — atomic test-and-set.
pub struct PortAllocator {
    reserved: DashSet<u16>,
    range_start: u16,
    range_end: u16,
}

impl PortAllocator {
    pub fn new(range_start: u16, range_end: u16) -> Self {
        Self {
            reserved: DashSet::new(),
            range_start,
            range_end,
        }
    }

    /// Atomically reserve the next available port.
    pub fn reserve(&self) -> Option<u16> {
        (self.range_start..=self.range_end).find(|&port| self.reserved.insert(port))
    }

    /// Pre-reserve a known port (e.g. fixed ports for default user).
    pub fn reserve_specific(&self, port: u16) -> bool {
        self.reserved.insert(port)
    }

    /// Release a port when a VM stops or fails.
    pub fn release(&self, port: u16) {
        self.reserved.remove(&port);
    }
}

/// Per-user sandbox registry (ADR-0022: DashMap for per-user concurrency).
pub struct SandboxRegistry {
    runtime_ctl: String,
    idle_timeout: Duration,
    /// user_id -> role/branch runtime entries. DashMap for per-shard locking.
    entries: DashMap<String, UserSandboxes>,
    live_port: u16,
    dev_port: u16,
    /// Atomic port allocator — eliminates TOCTOU in port assignment.
    port_allocator: PortAllocator,
    provider_gateway_base_url: Option<String>,
    provider_gateway_token: Option<String>,
    /// Configurable hard ceiling for concurrent VMs (ADR-0022).
    max_concurrent_vms: usize,
    /// When set, use systemd unit templates instead of bash runtime-ctl (ADR-0017).
    systemd_lifecycle: Option<SystemdLifecycle>,
}

impl SandboxRegistry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runtime_ctl: String,
        live_port: u16,
        dev_port: u16,
        branch_port_start: u16,
        branch_port_end: u16,
        idle_timeout: Duration,
        provider_gateway_base_url: Option<String>,
        provider_gateway_token: Option<String>,
    ) -> Arc<Self> {
        let systemd_lifecycle = SystemdLifecycle::from_env();
        if systemd_lifecycle.is_some() {
            info!("ADR-0017: systemd lifecycle manager available");
        }

        let max_concurrent_vms = std::env::var("CHOIR_MAX_VMS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200);

        let port_allocator = PortAllocator::new(branch_port_start, branch_port_end);
        // Pre-reserve fixed ports so they're never handed out dynamically.
        port_allocator.reserve_specific(live_port);
        port_allocator.reserve_specific(dev_port);

        Arc::new(Self {
            runtime_ctl,
            idle_timeout,
            entries: DashMap::new(),
            live_port,
            dev_port,
            port_allocator,
            provider_gateway_base_url,
            provider_gateway_token,
            max_concurrent_vms,
            systemd_lifecycle,
        })
    }

    /// Ensure the live sandbox is running at startup.
    /// Called once after hypervisor boot so the VM is ready before the first request.
    pub async fn boot_live_sandbox(self: &Arc<Self>) {
        if self.runtime_ctl.trim().is_empty() {
            return;
        }
        info!("booting live sandbox at startup");
        match self.ensure_running("default", SandboxRole::Live).await {
            Ok(port) => info!(port, "live sandbox ready"),
            Err(e) => tracing::error!("failed to boot live sandbox at startup: {e}"),
        }
    }

    /// Return the fixed port for a role, used only for the "default" bootstrap user.
    fn port_for_default(&self, role: SandboxRole) -> u16 {
        match role {
            SandboxRole::Live => self.live_port,
            SandboxRole::Dev => self.dev_port,
        }
    }

    async fn is_port_ready(port: u16) -> bool {
        TcpStream::connect(format!("127.0.0.1:{port}"))
            .await
            .is_ok()
    }

    /// ADR-0022: Dynamic VM cap based on resource usage.
    /// Per-VM memory is not predictable (varies with workload, KSM sharing,
    /// app mix), so we don't try to estimate it. The hard cap is the primary
    /// limit; memory check is a safety net against OOM.
    fn effective_max_vms(&self) -> usize {
        let hard_cap = self.max_concurrent_vms;
        match read_available_memory_mb() {
            Some(avail) if avail < MIN_AVAILABLE_MB => 0,
            _ => hard_cap,
        }
    }

    /// Start a sandbox for the given user + role.
    /// No-op if one is already running.
    /// ADR-0022: DashMap for per-user concurrency, boot coalescing via Starting status,
    /// PortAllocator for atomic port reservation, no hot-path readiness probe.
    pub async fn ensure_running(
        self: &Arc<Self>,
        user_id: &str,
        role: SandboxRole,
    ) -> anyhow::Result<u16> {
        // Phase A: Check existing entry (brief DashMap guard, never held across .await).
        {
            let mut user_map = self.entries.entry(user_id.to_string()).or_default();
            if let Some(entry) = user_map.roles.get_mut(&role) {
                match &entry.status {
                    // ADR-0022 Phase 2: trust Running status, no readiness probe.
                    // If the sandbox crashed, proxy returns 502 and marks it Failed.
                    SandboxStatus::Running => {
                        entry.last_activity = Instant::now();
                        return Ok(entry.port);
                    }
                    // ADR-0022 Phase 1: join existing boot instead of spawning again.
                    SandboxStatus::Starting(rx) => {
                        let mut rx = rx.clone();
                        drop(user_map); // release DashMap guard before awaiting
                        let _ = rx.changed().await;
                        return match rx.borrow().clone() {
                            Ok(port) => Ok(port),
                            Err(e) => Err(anyhow::anyhow!("boot failed: {e}")),
                        };
                    }
                    SandboxStatus::Hibernated => {
                        info!(user_id, %role, "sandbox hibernated; will restore from snapshot");
                    }
                    SandboxStatus::Stopped | SandboxStatus::Failed => {
                        info!(user_id, %role, status = ?entry.status, "sandbox not running; will re-spawn");
                    }
                }
            }
        }
        // DashMap guard dropped — spawn_instance can take a long time.

        // ADR-0022: capacity gate with dynamic cap.
        let is_existing = self
            .entries
            .get(user_id)
            .and_then(|u| u.roles.get(&role).map(|_| ()))
            .is_some();
        if !is_existing {
            let running = count_running_vms(&self.entries);
            let effective_max = self.effective_max_vms();
            if running >= effective_max {
                return Err(anyhow::anyhow!(
                    "Server at capacity ({running}/{effective_max} VMs). \
                     Please try again shortly."
                ));
            }
            if let Some(avail_mb) = read_available_memory_mb() {
                if avail_mb < MIN_AVAILABLE_MB {
                    return Err(anyhow::anyhow!(
                        "Insufficient memory ({avail_mb} MB available). \
                         Please try again shortly."
                    ));
                }
            }
        }

        // ADR-0014: "default" user gets fixed ports, others get dynamic ports.
        let port = if user_id == "default" || user_id == "public" {
            self.port_for_default(role)
        } else {
            // Check if user already had a port assigned (e.g. after hibernation)
            let existing_port = self
                .entries
                .get(user_id)
                .and_then(|u| u.roles.get(&role).map(|e| e.port));

            if let Some(port) = existing_port {
                port
            } else {
                // ADR-0022: atomic port reservation via PortAllocator
                let port = self.port_allocator.reserve().ok_or_else(|| {
                    anyhow::anyhow!(
                        "no available ports in range {}-{}",
                        self.port_allocator.range_start,
                        self.port_allocator.range_end,
                    )
                })?;
                port
            }
        };

        // ADR-0022: Insert Starting placeholder with watch channel for boot coalescing.
        let (tx, rx) = watch::channel(Err("booting".to_string()));
        {
            let mut user_map = self.entries.entry(user_id.to_string()).or_default();
            // Clean up residual handle before re-spawning
            if let Some(entry) = user_map.roles.get_mut(&role) {
                let mut handle = entry.handle.take();
                if handle.is_some() {
                    drop(user_map); // release guard before async stop
                    self.stop_handle(user_id, None, &mut handle).await;
                    let mut user_map = self.entries.entry(user_id.to_string()).or_default();
                    user_map.roles.insert(
                        role,
                        SandboxEntry {
                            role: Some(role),
                            branch: None,
                            port,
                            status: SandboxStatus::Starting(rx.clone()),
                            last_activity: Instant::now(),
                            handle: None,
                        },
                    );
                } else {
                    user_map.roles.insert(
                        role,
                        SandboxEntry {
                            role: Some(role),
                            branch: None,
                            port,
                            status: SandboxStatus::Starting(rx.clone()),
                            last_activity: Instant::now(),
                            handle: None,
                        },
                    );
                }
            } else {
                user_map.roles.insert(
                    role,
                    SandboxEntry {
                        role: Some(role),
                        branch: None,
                        port,
                        status: SandboxStatus::Starting(rx),
                        last_activity: Instant::now(),
                        handle: None,
                    },
                );
            }
        }
        // DashMap guard dropped — now spawn (can take seconds).

        let runtime_name = if user_id == "default" || user_id == "public" {
            role.to_string()
        } else {
            format!("u-{}", &user_id[..8.min(user_id.len())])
        };
        let handle = match self
            .spawn_instance(user_id, &runtime_name, Some(role), None, port)
            .await
        {
            Ok(h) => {
                // Notify all waiters: boot succeeded
                let _ = tx.send(Ok(port));
                h
            }
            Err(e) => {
                // Notify all waiters: boot failed
                let _ = tx.send(Err(e.to_string()));
                let mut user_map = self.entries.entry(user_id.to_string()).or_default();
                if let Some(entry) = user_map.roles.get_mut(&role) {
                    entry.status = SandboxStatus::Failed;
                }
                // Release port on failure (unless it's a fixed port)
                if user_id != "default" && user_id != "public" {
                    self.port_allocator.release(port);
                }
                return Err(e);
            }
        };

        // Store the running entry.
        {
            let mut user_map = self.entries.entry(user_id.to_string()).or_default();
            user_map.roles.insert(
                role,
                SandboxEntry {
                    role: Some(role),
                    branch: None,
                    port,
                    status: SandboxStatus::Running,
                    last_activity: Instant::now(),
                    handle: Some(handle),
                },
            );
        }

        info!(user_id, %role, port, "sandbox started");
        Ok(port)
    }

    /// Start (or adopt) a branch runtime for a user.
    pub async fn ensure_branch_running(
        self: &Arc<Self>,
        user_id: &str,
        branch: &str,
    ) -> anyhow::Result<u16> {
        validate_branch_name(branch)?;

        // Check existing entry (brief DashMap guard).
        {
            let mut user_map = self.entries.entry(user_id.to_string()).or_default();
            if let Some(entry) = user_map.branches.get_mut(branch) {
                match &entry.status {
                    SandboxStatus::Running => {
                        entry.last_activity = Instant::now();
                        return Ok(entry.port);
                    }
                    SandboxStatus::Starting(rx) => {
                        let mut rx = rx.clone();
                        drop(user_map);
                        let _ = rx.changed().await;
                        return match rx.borrow().clone() {
                            Ok(port) => Ok(port),
                            Err(e) => Err(anyhow::anyhow!("boot failed: {e}")),
                        };
                    }
                    SandboxStatus::Hibernated => {
                        info!(
                            user_id,
                            branch, "branch sandbox hibernated; will restore from snapshot"
                        );
                    }
                    SandboxStatus::Stopped | SandboxStatus::Failed => {
                        info!(user_id, branch, status = ?entry.status, "branch sandbox not running; will re-spawn");
                    }
                }
            }
        }
        // DashMap guard dropped.

        // Check for existing port or allocate new one.
        let port = {
            let existing_port = self
                .entries
                .get(user_id)
                .and_then(|u| u.branches.get(branch).map(|e| e.port));

            if let Some(port) = existing_port {
                port
            } else {
                self.port_allocator.reserve().ok_or_else(|| {
                    anyhow::anyhow!(
                        "no available ports in range {}-{}",
                        self.port_allocator.range_start,
                        self.port_allocator.range_end,
                    )
                })?
            }
        };

        // Insert Starting placeholder for boot coalescing.
        let (tx, rx) = watch::channel(Err("booting".to_string()));
        {
            let mut user_map = self.entries.entry(user_id.to_string()).or_default();
            // Clean up residual handle
            if let Some(entry) = user_map.branches.get_mut(branch) {
                entry.handle.take();
            }
            user_map.branches.insert(
                branch.to_string(),
                SandboxEntry {
                    role: None,
                    branch: Some(branch.to_string()),
                    port,
                    status: SandboxStatus::Starting(rx),
                    last_activity: Instant::now(),
                    handle: None,
                },
            );
        }

        let runtime_name = format!("branch-{branch}");
        let handle = match self
            .spawn_instance(user_id, &runtime_name, None, Some(branch), port)
            .await
        {
            Ok(h) => {
                let _ = tx.send(Ok(port));
                h
            }
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                if let Some(mut user_map) = self.entries.get_mut(user_id) {
                    if let Some(entry) = user_map.branches.get_mut(branch) {
                        entry.status = SandboxStatus::Failed;
                    }
                }
                self.port_allocator.release(port);
                return Err(e);
            }
        };

        {
            let mut user_map = self.entries.entry(user_id.to_string()).or_default();
            user_map.branches.insert(
                branch.to_string(),
                SandboxEntry {
                    role: None,
                    branch: Some(branch.to_string()),
                    port,
                    status: SandboxStatus::Running,
                    last_activity: Instant::now(),
                    handle: Some(handle),
                },
            );
        }

        info!(user_id, branch, port, "branch sandbox started");
        Ok(port)
    }

    /// Stop a sandbox for the given user + role.
    pub async fn stop(self: &Arc<Self>, user_id: &str, role: SandboxRole) -> anyhow::Result<()> {
        // Extract handle under brief guard, then stop without holding it.
        let mut handle = None;
        let mut port = None;
        if let Some(mut user_map) = self.entries.get_mut(user_id) {
            if let Some(entry) = user_map.roles.get_mut(&role) {
                handle = entry.handle.take();
                port = Some(entry.port);
                entry.status = SandboxStatus::Stopped;
            }
        }
        if handle.is_some() {
            self.stop_handle(user_id, None, &mut handle).await;
            if let Some(p) = port {
                self.port_allocator.release(p);
            }
            info!(user_id, %role, "sandbox stopped");
        }
        Ok(())
    }

    /// Hibernate a sandbox: pause + snapshot VM state, preserving it for fast restore.
    pub async fn hibernate(
        self: &Arc<Self>,
        user_id: &str,
        role: SandboxRole,
    ) -> anyhow::Result<()> {
        let handle = {
            let mut user_map = self
                .entries
                .get_mut(user_id)
                .ok_or_else(|| anyhow::anyhow!("user not found"))?;
            let entry = user_map
                .roles
                .get_mut(&role)
                .ok_or_else(|| anyhow::anyhow!("role not found"))?;
            if !matches!(entry.status, SandboxStatus::Running) {
                return Err(anyhow::anyhow!("sandbox is not running"));
            }
            entry.status = SandboxStatus::Hibernated;
            entry.handle.take()
        };
        // DashMap guard dropped — do async hibernate work.
        if let Some(handle) = handle {
            let mut handle = Some(handle);
            self.hibernate_handle(user_id, None, &mut handle).await;
            info!(user_id, %role, "sandbox hibernated");
        }
        Ok(())
    }

    /// Stop a branch runtime for the given user.
    pub async fn stop_branch(self: &Arc<Self>, user_id: &str, branch: &str) -> anyhow::Result<()> {
        let mut handle = None;
        let mut port = None;
        if let Some(mut user_map) = self.entries.get_mut(user_id) {
            if let Some(entry) = user_map.branches.get_mut(branch) {
                handle = entry.handle.take();
                port = Some(entry.port);
                entry.status = SandboxStatus::Stopped;
            }
        }
        if handle.is_some() {
            self.stop_handle(user_id, Some(branch), &mut handle).await;
            if let Some(p) = port {
                self.port_allocator.release(p);
            }
            info!(user_id, branch, "branch sandbox stopped");
        }
        Ok(())
    }

    /// Swap the Live and Dev roles for a user (promotion).
    pub async fn swap_roles(self: &Arc<Self>, user_id: &str) -> anyhow::Result<()> {
        let mut user_map = self.entries.entry(user_id.to_string()).or_default();

        let live = user_map.roles.remove(&SandboxRole::Live);
        let dev = user_map.roles.remove(&SandboxRole::Dev);

        if let Some(mut l) = live {
            l.role = Some(SandboxRole::Dev);
            user_map.roles.insert(SandboxRole::Dev, l);
        }
        if let Some(mut d) = dev {
            d.role = Some(SandboxRole::Live);
            user_map.roles.insert(SandboxRole::Live, d);
        }

        info!(user_id, "sandbox roles swapped (dev promoted to live)");
        Ok(())
    }

    /// Touch the activity timestamp for a running sandbox.
    pub async fn touch_activity(&self, user_id: &str, role: SandboxRole) {
        if let Some(mut user_map) = self.entries.get_mut(user_id) {
            if let Some(entry) = user_map.roles.get_mut(&role) {
                if matches!(entry.status, SandboxStatus::Running) {
                    entry.last_activity = Instant::now();
                }
            }
        }
    }

    /// Return the port for a running sandbox role, if any.
    pub async fn port_of(&self, user_id: &str, role: SandboxRole) -> Option<u16> {
        let mut user_map = self.entries.get_mut(user_id)?;
        let entry = user_map.roles.get_mut(&role)?;
        if matches!(entry.status, SandboxStatus::Running) {
            entry.last_activity = Instant::now();
            Some(entry.port)
        } else {
            None
        }
    }

    /// Return the port for a running branch sandbox, if any.
    pub async fn branch_port_of(&self, user_id: &str, branch: &str) -> Option<u16> {
        let mut user_map = self.entries.get_mut(user_id)?;
        let entry = user_map.branches.get_mut(branch)?;
        if matches!(entry.status, SandboxStatus::Running) {
            entry.last_activity = Instant::now();
            Some(entry.port)
        } else {
            None
        }
    }

    /// ADR-0022 Phase 2: Mark a sandbox as failed (called by proxy on 502).
    pub async fn mark_failed(&self, user_id: &str, role: SandboxRole) {
        if let Some(mut user_map) = self.entries.get_mut(user_id) {
            if let Some(entry) = user_map.roles.get_mut(&role) {
                if matches!(entry.status, SandboxStatus::Running) {
                    warn!(user_id, %role, "marking sandbox as failed (proxy 502)");
                    entry.status = SandboxStatus::Failed;
                }
            }
        }
    }

    /// Snapshot of all sandbox statuses for the status endpoint.
    pub async fn snapshot(&self) -> Vec<SandboxSnapshot> {
        let mut out = Vec::new();
        for entry in self.entries.iter() {
            let user_id = entry.key();
            let user_map = entry.value();
            for e in user_map.roles.values() {
                out.push(SandboxSnapshot {
                    user_id: user_id.clone(),
                    role: e.role,
                    branch: e.branch.clone(),
                    port: e.port,
                    status: e.status.clone(),
                    idle_secs: e.last_activity.elapsed().as_secs(),
                });
            }
            for e in user_map.branches.values() {
                out.push(SandboxSnapshot {
                    user_id: user_id.clone(),
                    role: e.role,
                    branch: e.branch.clone(),
                    port: e.port,
                    status: e.status.clone(),
                    idle_secs: e.last_activity.elapsed().as_secs(),
                });
            }
        }
        out
    }

    /// ADR-0018: Compute effective idle timeout based on memory pressure.
    /// More aggressive hibernation as available memory shrinks.
    fn effective_idle_timeout(&self) -> Duration {
        match read_memory_percent_available() {
            Some(pct) if pct > 60 => self.idle_timeout, // normal: configured (30 min)
            Some(pct) if pct > 30 => Duration::from_secs(300), // warning: 5 min
            Some(pct) if pct > 15 => Duration::from_secs(30), // high: 30 sec
            Some(_) => Duration::from_secs(0),          // critical: immediate
            None => self.idle_timeout,                  // fallback
        }
    }

    /// Background task: hibernate idle sandboxes (ADR-0018 + ADR-0022).
    /// ADR-0022: collect-then-execute pattern — never hold DashMap guard across async hibernate.
    pub async fn run_idle_watchdog(self: Arc<Self>) {
        loop {
            sleep(Duration::from_secs(30)).await;
            let timeout = self.effective_idle_timeout();
            let mem_pct = read_memory_percent_available().unwrap_or(100);

            if mem_pct <= 60 {
                info!(
                    mem_pct,
                    timeout_secs = timeout.as_secs(),
                    "idle watchdog: memory pressure detected"
                );
            }

            // Phase 1: Collect candidates under brief per-shard read locks.
            // (user_id, is_branch, branch_name_or_empty, idle_duration)
            let mut candidates: Vec<(String, bool, String, Duration)> = Vec::new();

            for entry in self.entries.iter() {
                let user_id = entry.key().clone();
                let user_map = entry.value();
                for (role, sandbox) in &user_map.roles {
                    if matches!(sandbox.status, SandboxStatus::Running) {
                        let idle = sandbox.last_activity.elapsed();
                        if mem_pct < 15 || idle >= timeout {
                            candidates.push((user_id.clone(), false, role.to_string(), idle));
                        }
                    }
                }
                for (branch, sandbox) in &user_map.branches {
                    if matches!(sandbox.status, SandboxStatus::Running) {
                        let idle = sandbox.last_activity.elapsed();
                        if mem_pct < 15 || idle >= timeout {
                            candidates.push((user_id.clone(), true, branch.clone(), idle));
                        }
                    }
                }
            }
            // All DashMap guards dropped here.

            if candidates.is_empty() {
                continue;
            }

            // At critical pressure, sort by idle duration and limit to 5.
            if mem_pct < 15 {
                candidates.sort_by(|a, b| b.3.cmp(&a.3));
                candidates.truncate(5);
                warn!(
                    mem_pct,
                    count = candidates.len(),
                    "CRITICAL memory pressure — force-hibernating least-active VMs"
                );
            }

            // Phase 2: Execute hibernations without holding DashMap guards.
            for (user_id, is_branch, key, idle) in &candidates {
                // Brief re-acquire to extract handle and update status.
                let mut handle = None;
                if let Some(mut user_map) = self.entries.get_mut(user_id) {
                    if *is_branch {
                        if let Some(entry) = user_map.branches.get_mut(key.as_str()) {
                            if matches!(entry.status, SandboxStatus::Running) {
                                handle = entry.handle.take();
                                entry.status = SandboxStatus::Hibernated;
                            }
                        }
                    } else {
                        let role = if key == "live" {
                            SandboxRole::Live
                        } else {
                            SandboxRole::Dev
                        };
                        if let Some(entry) = user_map.roles.get_mut(&role) {
                            if matches!(entry.status, SandboxStatus::Running) {
                                handle = entry.handle.take();
                                entry.status = SandboxStatus::Hibernated;
                            }
                        }
                    }
                }
                // Guard dropped — now do async hibernate work.
                if handle.is_some() {
                    warn!(
                        user_id,
                        key,
                        idle_secs = idle.as_secs(),
                        mem_pct,
                        "sandbox idle timeout — hibernating"
                    );
                    self.hibernate_handle(user_id, None, &mut handle).await;
                }
            }
        }
    }

    async fn spawn_instance(
        &self,
        user_id: &str,
        runtime_name: &str,
        role: Option<SandboxRole>,
        branch: Option<&str>,
        port: u16,
    ) -> anyhow::Result<SandboxHandle> {
        self.spawn_runtime(user_id, runtime_name, role, branch, port)
            .await
    }

    fn runtime_ctl_args(
        action: &str,
        user_id: &str,
        runtime_name: &str,
        role: Option<SandboxRole>,
        branch: Option<&str>,
        port: u16,
    ) -> Vec<String> {
        let mut args = vec![
            action.to_string(),
            "--user-id".to_string(),
            user_id.to_string(),
            "--runtime".to_string(),
            runtime_name.to_string(),
            "--port".to_string(),
            port.to_string(),
        ];

        if let Some(role) = role {
            args.push("--role".to_string());
            args.push(role.to_string());
        }
        if let Some(branch) = branch {
            args.push("--branch".to_string());
            args.push(branch.to_string());
        }

        args
    }

    async fn run_runtime_ctl(
        &self,
        action: &str,
        user_id: &str,
        runtime_name: &str,
        role: Option<SandboxRole>,
        branch: Option<&str>,
        port: u16,
    ) -> anyhow::Result<()> {
        let ctl_path = self.runtime_ctl.trim();
        if ctl_path.is_empty() {
            return Err(anyhow::anyhow!(
                "runtime backend is configured but SANDBOX_VFKIT_CTL is missing"
            ));
        }

        let args = Self::runtime_ctl_args(action, user_id, runtime_name, role, branch, port);
        let mut cmd = Command::new(ctl_path);
        cmd.args(args)
            .env("CHOIR_SANDBOX_USER_ID", user_id)
            .env("CHOIR_SANDBOX_RUNTIME", runtime_name)
            .env("CHOIR_SANDBOX_PORT", port.to_string());

        if let Some(role) = role {
            cmd.env("CHOIR_SANDBOX_ROLE", role.to_string());
        }
        if let Some(branch) = branch {
            cmd.env("CHOIR_SANDBOX_BRANCH", branch);
        }
        if let Some(base_url) = self.provider_gateway_base_url.as_ref() {
            cmd.env("CHOIR_PROVIDER_GATEWAY_BASE_URL", base_url);
        }
        if let Some(token) = self.provider_gateway_token.as_ref() {
            cmd.env("CHOIR_PROVIDER_GATEWAY_TOKEN", token);
        }
        if let Ok(frontend_dist) = std::env::var("FRONTEND_DIST") {
            cmd.env("FRONTEND_DIST", frontend_dist);
        }

        // Use spawn + wait instead of output() to avoid blocking forever.
        // The runtime_ctl script spawns background daemons (virtiofsd, cloud-hypervisor,
        // socat) that inherit stdout/stderr pipes. cmd.output() waits for ALL pipe
        // readers to close, which never happens while daemons are alive.
        // By using Stdio::null we avoid this pipe inheritance deadlock entirely.
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());

        let status = cmd.status().await?;
        if status.success() {
            return Ok(());
        }

        Err(anyhow::anyhow!(
            "runtime ctl failed action={action} runtime={runtime_name} code={:?}",
            status.code(),
        ))
    }

    async fn spawn_runtime(
        &self,
        user_id: &str,
        runtime_name: &str,
        role: Option<SandboxRole>,
        branch: Option<&str>,
        port: u16,
    ) -> anyhow::Result<SandboxHandle> {
        // ADR-0017: prefer systemd lifecycle when available
        if let Some(lifecycle) = &self.systemd_lifecycle {
            info!(
                user_id,
                runtime_name, port, "using systemd lifecycle (ADR-0017)"
            );
            lifecycle
                .ensure(
                    runtime_name,
                    user_id,
                    port,
                    self.provider_gateway_token.as_deref(),
                )
                .await?;
        } else {
            self.run_runtime_ctl("ensure", user_id, runtime_name, role, branch, port)
                .await?;
        }

        // Poll for host-facing runtime readiness after runtime control succeeds.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
        loop {
            if tokio::time::Instant::now() >= deadline {
                error!(
                    runtime = runtime_name,
                    port, "sandbox runtime did not become ready within 45s"
                );
                return Err(anyhow::anyhow!(
                    "sandbox runtime readiness timeout on port {port}"
                ));
            }
            if Self::is_port_ready(port).await {
                info!(
                    runtime = runtime_name,
                    port, "sandbox runtime port is ready"
                );
                break;
            }
            sleep(Duration::from_millis(200)).await;
        }

        Ok(SandboxHandle::RuntimeCtl(RuntimeCtlHandle {
            user_id: user_id.to_string(),
            runtime_name: runtime_name.to_string(),
            role,
            branch: branch.map(ToString::to_string),
            port,
        }))
    }

    async fn stop_handle(
        &self,
        user_id: &str,
        branch: Option<&str>,
        handle: &mut Option<SandboxHandle>,
    ) {
        let Some(handle) = handle.take() else {
            return;
        };

        match handle {
            SandboxHandle::RuntimeCtl(handle) => {
                // ADR-0017: prefer systemd lifecycle when available
                if let Some(lifecycle) = &self.systemd_lifecycle {
                    if let Err(e) = lifecycle.stop(&handle.runtime_name, &handle.user_id).await {
                        warn!(
                            user_id,
                            runtime = handle.runtime_name,
                            "systemd stop failed: {e}"
                        );
                    }
                } else if let Err(e) = self
                    .run_runtime_ctl(
                        "stop",
                        &handle.user_id,
                        &handle.runtime_name,
                        handle.role,
                        handle.branch.as_deref(),
                        handle.port,
                    )
                    .await
                {
                    warn!(
                        user_id,
                        branch = ?branch,
                        runtime = handle.runtime_name,
                        "failed to stop sandbox runtime: {e}"
                    );
                }
            }
        }
    }

    async fn hibernate_handle(
        &self,
        user_id: &str,
        branch: Option<&str>,
        handle: &mut Option<SandboxHandle>,
    ) {
        let Some(handle) = handle.take() else {
            return;
        };

        match handle {
            SandboxHandle::RuntimeCtl(handle) => {
                // ADR-0017: prefer systemd lifecycle when available
                if let Some(lifecycle) = &self.systemd_lifecycle {
                    if let Err(e) = lifecycle
                        .hibernate(&handle.runtime_name, &handle.user_id)
                        .await
                    {
                        warn!(
                            user_id,
                            runtime = handle.runtime_name,
                            "systemd hibernate failed: {e}"
                        );
                    }
                } else if let Err(e) = self
                    .run_runtime_ctl(
                        "hibernate",
                        &handle.user_id,
                        &handle.runtime_name,
                        handle.role,
                        handle.branch.as_deref(),
                        handle.port,
                    )
                    .await
                {
                    warn!(
                        user_id,
                        branch = ?branch,
                        runtime = handle.runtime_name,
                        "failed to hibernate sandbox runtime: {e}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_branch_name, SandboxRegistry, SandboxRole};

    #[test]
    fn validates_branch_names() {
        assert!(validate_branch_name("feature_login").is_ok());
        assert!(validate_branch_name("feat-123.abc").is_ok());
        assert!(validate_branch_name("bad/branch").is_err());
        assert!(validate_branch_name("").is_err());
    }

    #[test]
    fn runtime_ctl_args_include_role_or_branch_targets() {
        let role_args = SandboxRegistry::runtime_ctl_args(
            "ensure",
            "user-a",
            "live",
            Some(SandboxRole::Live),
            None,
            8080,
        );
        assert_eq!(
            role_args,
            vec![
                "ensure",
                "--user-id",
                "user-a",
                "--runtime",
                "live",
                "--port",
                "8080",
                "--role",
                "live",
            ]
        );

        let branch_args = SandboxRegistry::runtime_ctl_args(
            "stop",
            "user-a",
            "branch-feature",
            None,
            Some("feature"),
            12000,
        );
        assert_eq!(
            branch_args,
            vec![
                "stop",
                "--user-id",
                "user-a",
                "--runtime",
                "branch-feature",
                "--port",
                "12000",
                "--branch",
                "feature",
            ]
        );
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SandboxSnapshot {
    pub user_id: String,
    pub role: Option<SandboxRole>,
    pub branch: Option<String>,
    pub port: u16,
    pub status: SandboxStatus,
    pub idle_secs: u64,
}

impl serde::Serialize for SandboxRole {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl serde::Serialize for SandboxStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let v = match self {
            SandboxStatus::Running => "running",
            SandboxStatus::Stopped => "stopped",
            SandboxStatus::Hibernated => "hibernated",
            SandboxStatus::Starting(_) => "starting",
            SandboxStatus::Failed => "failed",
        };
        s.serialize_str(v)
    }
}
