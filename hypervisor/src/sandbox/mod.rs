use std::{
    collections::HashMap,
    process::Stdio,
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::{
    net::TcpStream,
    process::{Child, Command},
    sync::Mutex,
    time::sleep,
};
use tracing::{error, info, warn};

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
    /// Process exited unexpectedly.
    Failed,
}

pub struct SandboxEntry {
    pub role: SandboxRole,
    pub port: u16,
    pub status: SandboxStatus,
    pub last_activity: Instant,
    pub child: Option<Child>,
}

/// Per-user sandbox registry.  
/// Currently single-user; keyed by user_id for future multi-user expansion.
pub struct SandboxRegistry {
    binary: String,
    idle_timeout: Duration,
    /// user_id → role → entry
    entries: Mutex<HashMap<String, HashMap<SandboxRole, SandboxEntry>>>,
    live_port: u16,
    dev_port: u16,
}

impl SandboxRegistry {
    pub fn new(binary: String, live_port: u16, dev_port: u16, idle_timeout: Duration) -> Arc<Self> {
        Arc::new(Self {
            binary,
            idle_timeout,
            entries: Mutex::new(HashMap::new()),
            live_port,
            dev_port,
        })
    }

    fn port_for(&self, role: SandboxRole) -> u16 {
        match role {
            SandboxRole::Live => self.live_port,
            SandboxRole::Dev => self.dev_port,
        }
    }

    /// Start a sandbox for the given user + role.  
    /// No-op if one is already running.
    pub async fn ensure_running(
        self: &Arc<Self>,
        user_id: &str,
        role: SandboxRole,
    ) -> anyhow::Result<u16> {
        let mut entries = self.entries.lock().await;
        let user_map = entries.entry(user_id.to_string()).or_default();

        if let Some(entry) = user_map.get_mut(&role) {
            if entry.status == SandboxStatus::Running {
                entry.last_activity = Instant::now();
                return Ok(entry.port);
            }
        }

        let port = self.port_for(role);
        let child = match self.spawn_process(role, port).await {
            Ok(c) => c,
            Err(e) => {
                // Record the failure so callers see Failed instead of Stopped.
                user_map.insert(
                    role,
                    SandboxEntry {
                        role,
                        port,
                        status: SandboxStatus::Failed,
                        last_activity: Instant::now(),
                        child: None,
                    },
                );
                return Err(e);
            }
        };

        user_map.insert(
            role,
            SandboxEntry {
                role,
                port,
                status: SandboxStatus::Running,
                last_activity: Instant::now(),
                child: Some(child),
            },
        );

        info!(user_id, %role, port, "sandbox started");
        Ok(port)
    }

    /// Stop a sandbox for the given user + role.
    pub async fn stop(self: &Arc<Self>, user_id: &str, role: SandboxRole) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().await;
        if let Some(user_map) = entries.get_mut(user_id) {
            if let Some(entry) = user_map.get_mut(&role) {
                if let Some(child) = entry.child.as_mut() {
                    child.kill().await.ok();
                }
                entry.status = SandboxStatus::Stopped;
                entry.child = None;
                info!(user_id, %role, "sandbox stopped");
            }
        }
        Ok(())
    }

    /// Swap the Live and Dev roles for a user (promotion).
    /// The processes keep running; only the port mapping in the registry changes.
    pub async fn swap_roles(self: &Arc<Self>, user_id: &str) -> anyhow::Result<()> {
        let mut entries = self.entries.lock().await;
        let user_map = entries.entry(user_id.to_string()).or_default();

        let live = user_map.remove(&SandboxRole::Live);
        let dev = user_map.remove(&SandboxRole::Dev);

        if let Some(mut l) = live {
            l.role = SandboxRole::Dev;
            user_map.insert(SandboxRole::Dev, l);
        }
        if let Some(mut d) = dev {
            d.role = SandboxRole::Live;
            user_map.insert(SandboxRole::Live, d);
        }

        info!(user_id, "sandbox roles swapped (dev promoted to live)");
        Ok(())
    }

    /// Return the port for a running sandbox, if any.
    pub async fn port_of(&self, user_id: &str, role: SandboxRole) -> Option<u16> {
        let mut entries = self.entries.lock().await;
        let entry = entries.get_mut(user_id)?.get_mut(&role)?;
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
            for (role, entry) in user_map.iter() {
                out.push(SandboxSnapshot {
                    user_id: user_id.clone(),
                    role: *role,
                    port: entry.port,
                    status: entry.status.clone(),
                    idle_secs: entry.last_activity.elapsed().as_secs(),
                });
            }
        }
        out
    }

    /// Background task: kill idle sandboxes.
    pub async fn run_idle_watchdog(self: Arc<Self>) {
        loop {
            sleep(Duration::from_secs(60)).await;
            let mut entries = self.entries.lock().await;
            for (user_id, user_map) in entries.iter_mut() {
                for (role, entry) in user_map.iter_mut() {
                    if entry.status == SandboxStatus::Running
                        && entry.last_activity.elapsed() >= self.idle_timeout
                    {
                        warn!(user_id, %role, "sandbox idle timeout — stopping");
                        if let Some(child) = entry.child.as_mut() {
                            child.kill().await.ok();
                        }
                        entry.status = SandboxStatus::Stopped;
                        entry.child = None;
                    }
                }
            }
        }
    }

    async fn spawn_process(&self, role: SandboxRole, port: u16) -> anyhow::Result<Child> {
        // Brief wait for the port to become available after a prior process exits.
        sleep(Duration::from_millis(200)).await;

        let child = Command::new(&self.binary)
            .env("PORT", port.to_string())
            .env("DATABASE_URL", format!("sqlite:./data/sandbox_{role}.db"))
            .env("SQLX_OFFLINE", "true")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| {
                error!(%role, port, binary = %self.binary, "failed to spawn sandbox: {e}");
                e
            })?;

        // TCP readiness probe: poll until the sandbox accepts connections or deadline.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            if tokio::time::Instant::now() >= deadline {
                error!(%role, port, "sandbox did not become ready within 30s");
                return Err(anyhow::anyhow!("sandbox readiness timeout on port {port}"));
            }
            match TcpStream::connect(format!("127.0.0.1:{port}")).await {
                Ok(_) => {
                    info!(%role, port, "sandbox port is ready");
                    break;
                }
                Err(_) => sleep(Duration::from_millis(100)).await,
            }
        }

        Ok(child)
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SandboxSnapshot {
    pub user_id: String,
    pub role: SandboxRole,
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
            SandboxStatus::Failed => "failed",
        };
        s.serialize_str(v)
    }
}
