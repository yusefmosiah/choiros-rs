pub mod systemd;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{net::TcpStream, process::Command, sync::Mutex, time::sleep};
use tracing::{error, info, warn};

use self::systemd::SystemdLifecycle;

// ── Memory pressure helpers (ADR-0018) ──────────────────────────────────────

/// Read MemAvailable from /proc/meminfo (in MB).
fn read_available_memory_mb() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb: u64 = rest.trim().split_whitespace().next()?.parse().ok()?;
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
            total_kb = rest.trim().split_whitespace().next()?.parse().ok()?;
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            avail_kb = rest.trim().split_whitespace().next()?.parse().ok()?;
        }
    }
    if total_kb == 0 {
        return None;
    }
    Some(avail_kb * 100 / total_kb)
}

/// Count running VMs across all users.
fn count_running_vms(entries: &HashMap<String, UserSandboxes>) -> usize {
    entries
        .values()
        .map(|u| {
            u.roles
                .values()
                .filter(|e| e.status == SandboxStatus::Running)
                .count()
                + u.branches
                    .values()
                    .filter(|e| e.status == SandboxStatus::Running)
                    .count()
        })
        .sum()
}

const MAX_CONCURRENT_VMS: usize = 50;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SandboxStatus {
    Running,
    Stopped,
    /// VM snapshot saved to disk — fast restore available via `runtime_ctl ensure`.
    Hibernated,
    /// Process exited unexpectedly.
    Failed,
}

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

/// Per-user sandbox registry.
pub struct SandboxRegistry {
    runtime_ctl: String,
    idle_timeout: Duration,
    /// user_id -> role/branch runtime entries
    entries: Mutex<HashMap<String, UserSandboxes>>,
    live_port: u16,
    dev_port: u16,
    branch_port_start: u16,
    branch_port_end: u16,
    provider_gateway_base_url: Option<String>,
    provider_gateway_token: Option<String>,
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
        Arc::new(Self {
            runtime_ctl,
            idle_timeout,
            entries: Mutex::new(HashMap::new()),
            live_port,
            dev_port,
            branch_port_start,
            branch_port_end,
            provider_gateway_base_url,
            provider_gateway_token,
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

    /// Allocate a dynamic port from the port range for per-user VMs.
    /// Tracks ALL allocated ports (not just Running) to prevent collisions.
    async fn allocate_port(&self, entries: &HashMap<String, UserSandboxes>) -> anyhow::Result<u16> {
        let mut used = HashSet::new();
        for user_map in entries.values() {
            for entry in user_map.roles.values() {
                used.insert(entry.port);
            }
            for entry in user_map.branches.values() {
                used.insert(entry.port);
            }
        }

        for port in self.branch_port_start..=self.branch_port_end {
            if used.contains(&port) {
                continue;
            }
            if Self::is_port_ready(port).await {
                continue;
            }
            return Ok(port);
        }

        Err(anyhow::anyhow!(
            "no available ports in range {}-{}",
            self.branch_port_start,
            self.branch_port_end
        ))
    }

    /// Start a sandbox for the given user + role.
    /// No-op if one is already running.
    pub async fn ensure_running(
        self: &Arc<Self>,
        user_id: &str,
        role: SandboxRole,
    ) -> anyhow::Result<u16> {
        // Check if already running (short lock).
        {
            let mut entries = self.entries.lock().await;
            let user_map = entries.entry(user_id.to_string()).or_default();

            if let Some(entry) = user_map.roles.get_mut(&role) {
                match entry.status {
                    SandboxStatus::Running => {
                        if Self::is_port_ready(entry.port).await {
                            entry.last_activity = Instant::now();
                            return Ok(entry.port);
                        }
                        warn!(
                            user_id,
                            %role,
                            port = entry.port,
                            "sandbox marked running but port is down; recycling runtime"
                        );
                        self.stop_handle(user_id, entry.branch.as_deref(), &mut entry.handle)
                            .await;
                        entry.status = SandboxStatus::Failed;
                    }
                    SandboxStatus::Hibernated => {
                        // VM snapshot exists on disk — runtime_ctl ensure will
                        // restore from it (fast resume) instead of cold booting.
                        info!(
                            user_id,
                            %role,
                            "sandbox hibernated; will restore from snapshot"
                        );
                    }
                    SandboxStatus::Stopped | SandboxStatus::Failed => {
                        // Clean up any residual handle before re-spawning.
                        self.stop_handle(user_id, entry.branch.as_deref(), &mut entry.handle)
                            .await;
                        info!(
                            user_id,
                            %role,
                            status = ?entry.status,
                            "sandbox not running; will re-spawn"
                        );
                    }
                }
            }
        }
        // Lock released — spawn_instance can take a long time (runtime_ctl + readiness poll).

        // ADR-0018: capacity gate — refuse new VMs when system is at limit.
        // Only checked for new spawns (existing Running/Hibernated VMs skip this).
        {
            let entries = self.entries.lock().await;
            let is_existing = entries
                .get(user_id)
                .and_then(|u| u.roles.get(&role))
                .is_some();
            if !is_existing {
                let running = count_running_vms(&entries);
                if running >= MAX_CONCURRENT_VMS {
                    return Err(anyhow::anyhow!(
                        "Server at capacity ({running}/{MAX_CONCURRENT_VMS} VMs). \
                         Please try again in 30 seconds."
                    ));
                }
                if let Some(avail_mb) = read_available_memory_mb() {
                    if avail_mb < MIN_AVAILABLE_MB {
                        return Err(anyhow::anyhow!(
                            "Insufficient memory ({avail_mb} MB available). \
                             Please try again in 30 seconds."
                        ));
                    }
                }
            }
        }

        // ADR-0014: "default" user gets fixed ports (bootstrap/unauthenticated),
        // all other users get dynamic ports for per-user VM isolation.
        let port = if user_id == "default" || user_id == "public" {
            let port = self.port_for_default(role);
            if Self::is_port_ready(port).await {
                warn!(
                    user_id,
                    %role,
                    port,
                    "role port already listening before ensure; delegating to runtime control for ownership validation"
                );
            }
            port
        } else {
            // Check if user already had a port assigned (e.g. after hibernation)
            let mut entries = self.entries.lock().await;
            let existing_port = entries
                .get(user_id)
                .and_then(|u| u.roles.get(&role))
                .map(|e| e.port);

            if let Some(port) = existing_port {
                drop(entries);
                port
            } else {
                let port = self.allocate_port(&entries).await?;
                // Insert placeholder immediately to prevent concurrent allocations
                // from claiming the same port during the slow spawn_instance() call.
                let user_map = entries.entry(user_id.to_string()).or_default();
                user_map.roles.insert(
                    role,
                    SandboxEntry {
                        role: Some(role),
                        branch: None,
                        port,
                        status: SandboxStatus::Stopped,
                        last_activity: Instant::now(),
                        handle: None,
                    },
                );
                drop(entries);
                port
            }
        };

        let runtime_name = if user_id == "default" || user_id == "public" {
            role.to_string()
        } else {
            // Per-user instance name: "u-{first 8 chars of user_id}"
            format!("u-{}", &user_id[..8.min(user_id.len())])
        };
        let handle = match self
            .spawn_instance(user_id, &runtime_name, Some(role), None, port)
            .await
        {
            Ok(h) => h,
            Err(e) => {
                let mut entries = self.entries.lock().await;
                let user_map = entries.entry(user_id.to_string()).or_default();
                user_map.roles.insert(
                    role,
                    SandboxEntry {
                        role: Some(role),
                        branch: None,
                        port,
                        status: SandboxStatus::Failed,
                        last_activity: Instant::now(),
                        handle: None,
                    },
                );
                return Err(e);
            }
        };

        // Re-acquire lock to store the running entry.
        let mut entries = self.entries.lock().await;
        let user_map = entries.entry(user_id.to_string()).or_default();
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

        let mut entries = self.entries.lock().await;
        {
            let user_map = entries.entry(user_id.to_string()).or_default();
            if let Some(entry) = user_map.branches.get_mut(branch) {
                match entry.status {
                    SandboxStatus::Running => {
                        if Self::is_port_ready(entry.port).await {
                            entry.last_activity = Instant::now();
                            return Ok(entry.port);
                        }
                        warn!(
                            user_id,
                            branch,
                            port = entry.port,
                            "branch sandbox marked running but port is down; recycling"
                        );
                        self.stop_handle(user_id, entry.branch.as_deref(), &mut entry.handle)
                            .await;
                        entry.status = SandboxStatus::Failed;
                    }
                    SandboxStatus::Hibernated => {
                        info!(
                            user_id,
                            branch, "branch sandbox hibernated; will restore from snapshot"
                        );
                    }
                    SandboxStatus::Stopped | SandboxStatus::Failed => {
                        self.stop_handle(user_id, entry.branch.as_deref(), &mut entry.handle)
                            .await;
                        info!(
                            user_id,
                            branch,
                            status = ?entry.status,
                            "branch sandbox not running; will re-spawn"
                        );
                    }
                }
            }
        }

        let port = self.allocate_port(&entries).await?;
        let user_map = entries.entry(user_id.to_string()).or_default();

        let runtime_name = format!("branch-{branch}");
        let handle = match self
            .spawn_instance(user_id, &runtime_name, None, Some(branch), port)
            .await
        {
            Ok(h) => h,
            Err(e) => {
                user_map.branches.insert(
                    branch.to_string(),
                    SandboxEntry {
                        role: None,
                        branch: Some(branch.to_string()),
                        port,
                        status: SandboxStatus::Failed,
                        last_activity: Instant::now(),
                        handle: None,
                    },
                );
                return Err(e);
            }
        };

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

        info!(user_id, branch, port, "branch sandbox started");
        Ok(port)
    }

    /// Stop a sandbox for the given user + role.
    pub async fn stop(self: &Arc<Self>, user_id: &str, role: SandboxRole) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().await;
        if let Some(user_map) = entries.get_mut(user_id) {
            if let Some(entry) = user_map.roles.get_mut(&role) {
                self.stop_handle(user_id, entry.branch.as_deref(), &mut entry.handle)
                    .await;
                entry.status = SandboxStatus::Stopped;
                info!(user_id, %role, "sandbox stopped");
            }
        }
        Ok(())
    }

    /// Hibernate a sandbox: pause + snapshot VM state, preserving it for fast restore.
    pub async fn hibernate(
        self: &Arc<Self>,
        user_id: &str,
        role: SandboxRole,
    ) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().await;
        if let Some(user_map) = entries.get_mut(user_id) {
            if let Some(entry) = user_map.roles.get_mut(&role) {
                if entry.status != SandboxStatus::Running {
                    return Err(anyhow::anyhow!("sandbox is not running"));
                }
                self.hibernate_handle(user_id, entry.branch.as_deref(), &mut entry.handle)
                    .await;
                entry.status = SandboxStatus::Hibernated;
                info!(user_id, %role, "sandbox hibernated");
            }
        }
        Ok(())
    }

    /// Stop a branch runtime for the given user.
    pub async fn stop_branch(self: &Arc<Self>, user_id: &str, branch: &str) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().await;
        if let Some(user_map) = entries.get_mut(user_id) {
            if let Some(entry) = user_map.branches.get_mut(branch) {
                self.stop_handle(user_id, Some(branch), &mut entry.handle)
                    .await;
                entry.status = SandboxStatus::Stopped;
                info!(user_id, branch, "branch sandbox stopped");
            }
        }
        Ok(())
    }

    /// Swap the Live and Dev roles for a user (promotion).
    /// The processes keep running; only the role mapping in the registry changes.
    pub async fn swap_roles(self: &Arc<Self>, user_id: &str) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().await;
        let user_map = entries.entry(user_id.to_string()).or_default();

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
    /// Called by the proxy on every request to prevent idle watchdog kills.
    pub async fn touch_activity(&self, user_id: &str, role: SandboxRole) {
        let mut entries = self.entries.lock().await;
        if let Some(user_map) = entries.get_mut(user_id) {
            if let Some(entry) = user_map.roles.get_mut(&role) {
                if entry.status == SandboxStatus::Running {
                    entry.last_activity = Instant::now();
                }
            }
        }
    }

    /// Return the port for a running sandbox role, if any.
    pub async fn port_of(&self, user_id: &str, role: SandboxRole) -> Option<u16> {
        let mut entries = self.entries.lock().await;
        let entry = entries.get_mut(user_id)?.roles.get_mut(&role)?;
        if entry.status == SandboxStatus::Running {
            entry.last_activity = Instant::now();
            Some(entry.port)
        } else {
            None
        }
    }

    /// Return the port for a running branch sandbox, if any.
    pub async fn branch_port_of(&self, user_id: &str, branch: &str) -> Option<u16> {
        let mut entries = self.entries.lock().await;
        let entry = entries.get_mut(user_id)?.branches.get_mut(branch)?;
        if entry.status == SandboxStatus::Running {
            entry.last_activity = Instant::now();
            Some(entry.port)
        } else {
            None
        }
    }

    /// Snapshot of all sandbox statuses for the status endpoint.
    pub async fn snapshot(&self) -> Vec<SandboxSnapshot> {
        let entries = self.entries.lock().await;
        let mut out = Vec::new();
        for (user_id, user_map) in entries.iter() {
            for entry in user_map.roles.values() {
                out.push(SandboxSnapshot {
                    user_id: user_id.clone(),
                    role: entry.role,
                    branch: entry.branch.clone(),
                    port: entry.port,
                    status: entry.status.clone(),
                    idle_secs: entry.last_activity.elapsed().as_secs(),
                });
            }
            for entry in user_map.branches.values() {
                out.push(SandboxSnapshot {
                    user_id: user_id.clone(),
                    role: entry.role,
                    branch: entry.branch.clone(),
                    port: entry.port,
                    status: entry.status.clone(),
                    idle_secs: entry.last_activity.elapsed().as_secs(),
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

    /// Background task: hibernate idle sandboxes (ADR-0018: memory-pressure-aware).
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

            let mut entries = self.entries.lock().await;

            // At critical pressure (<15%), force-hibernate the least-recently-active
            // VMs regardless of idle time, until we're above 15%.
            if mem_pct < 15 {
                let mut running: Vec<(&str, &str, Duration)> = Vec::new();
                for (user_id, user_map) in entries.iter() {
                    for entry in user_map.roles.values() {
                        if entry.status == SandboxStatus::Running {
                            running.push((user_id.as_str(), "role", entry.last_activity.elapsed()));
                        }
                    }
                    for entry in user_map.branches.values() {
                        if entry.status == SandboxStatus::Running {
                            running.push((
                                user_id.as_str(),
                                "branch",
                                entry.last_activity.elapsed(),
                            ));
                        }
                    }
                }
                // Sort by idle duration descending (most idle first)
                running.sort_by(|a, b| b.2.cmp(&a.2));

                // Hibernate up to 5 VMs per cycle to reclaim memory
                let to_hibernate = running.len().min(5);
                if to_hibernate > 0 {
                    warn!(
                        mem_pct,
                        to_hibernate,
                        "CRITICAL memory pressure — force-hibernating least-active VMs"
                    );
                }
                // Collect user_ids to hibernate (avoid borrow issues)
                let targets: Vec<String> = running
                    .iter()
                    .take(to_hibernate)
                    .map(|(uid, _, _)| uid.to_string())
                    .collect();
                for uid in &targets {
                    if let Some(user_map) = entries.get_mut(uid.as_str()) {
                        for entry in user_map.roles.values_mut() {
                            if entry.status == SandboxStatus::Running {
                                self.hibernate_handle(
                                    uid,
                                    entry.branch.as_deref(),
                                    &mut entry.handle,
                                )
                                .await;
                                entry.status = SandboxStatus::Hibernated;
                                break; // one per user per cycle
                            }
                        }
                    }
                }
            }

            // Normal idle timeout sweep (with pressure-adjusted threshold)
            for (user_id, user_map) in entries.iter_mut() {
                for entry in user_map.roles.values_mut() {
                    if entry.status == SandboxStatus::Running
                        && entry.last_activity.elapsed() >= timeout
                    {
                        warn!(
                            user_id,
                            role = ?entry.role,
                            idle_secs = entry.last_activity.elapsed().as_secs(),
                            mem_pct,
                            "sandbox idle timeout — hibernating"
                        );
                        self.hibernate_handle(user_id, entry.branch.as_deref(), &mut entry.handle)
                            .await;
                        entry.status = SandboxStatus::Hibernated;
                    }
                }

                for entry in user_map.branches.values_mut() {
                    if entry.status == SandboxStatus::Running
                        && entry.last_activity.elapsed() >= timeout
                    {
                        warn!(
                            user_id,
                            branch = ?entry.branch,
                            idle_secs = entry.last_activity.elapsed().as_secs(),
                            mem_pct,
                            "branch sandbox idle timeout — hibernating"
                        );
                        self.hibernate_handle(user_id, entry.branch.as_deref(), &mut entry.handle)
                            .await;
                        entry.status = SandboxStatus::Hibernated;
                    }
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
            SandboxStatus::Failed => "failed",
        };
        s.serialize_str(v)
    }
}
