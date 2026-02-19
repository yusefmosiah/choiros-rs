use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    /// Port the hypervisor listens on
    pub port: u16,
    /// Path to the sandbox binary
    pub sandbox_binary: String,
    /// Port the live sandbox listens on (hypervisor assigns this)
    pub sandbox_live_port: u16,
    /// Port the dev sandbox listens on
    pub sandbox_dev_port: u16,
    /// How long a sandbox can be idle before it is shut down
    pub sandbox_idle_timeout: Duration,
    /// Path to the hypervisor SQLite database
    pub database_url: String,
    /// RP ID for WebAuthn (must match the effective domain, no port)
    pub webauthn_rp_id: String,
    /// RP origin for WebAuthn (full https origin)
    pub webauthn_rp_origin: String,
    /// RP display name for WebAuthn
    pub webauthn_rp_name: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        Ok(Self {
            port: env_parse("HYPERVISOR_PORT", 9090)?,
            sandbox_binary: {
                // Default: workspace root /target/debug/sandbox (resolved at compile time).
                // The hypervisor may be launched from any directory, so use an absolute path.
                // Override with SANDBOX_BINARY env var.
                if let Ok(v) = std::env::var("SANDBOX_BINARY") {
                    v
                } else {
                    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from("."));
                    workspace_root
                        .join("target/debug/sandbox")
                        .to_string_lossy()
                        .to_string()
                }
            },
            sandbox_live_port: env_parse("SANDBOX_LIVE_PORT", 8080)?,
            sandbox_dev_port: env_parse("SANDBOX_DEV_PORT", 8081)?,
            sandbox_idle_timeout: Duration::from_secs(env_parse(
                "SANDBOX_IDLE_TIMEOUT_SECS",
                1800,
            )?),
            database_url: env_str("HYPERVISOR_DATABASE_URL", "sqlite:./data/hypervisor.db"),
            webauthn_rp_id: env_str("WEBAUTHN_RP_ID", "localhost"),
            webauthn_rp_origin: env_str("WEBAUTHN_RP_ORIGIN", "http://localhost:9090"),
            webauthn_rp_name: env_str("WEBAUTHN_RP_NAME", "ChoirOS"),
        })
    }
}

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(val) => val
            .parse::<T>()
            .map_err(|e| anyhow::anyhow!("Failed to parse env var {key}={val}: {e}")),
        Err(_) => Ok(default),
    }
}

/// Resolve the Dioxus frontend dist directory.
///
/// If `FRONTEND_DIST` is set, that value is used as-is.
/// Otherwise resolve from the workspace root so this works whether the
/// hypervisor is launched from repository root or from `hypervisor/`.
pub fn frontend_dist_from_env() -> String {
    if let Ok(path) = std::env::var("FRONTEND_DIST") {
        return path;
    }

    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    workspace_root
        .join("dioxus-desktop/target/dx/sandbox-ui/debug/web/public")
        .to_string_lossy()
        .to_string()
}
