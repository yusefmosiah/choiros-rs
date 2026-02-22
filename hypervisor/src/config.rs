use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxRuntime {
    Process,
}

impl SandboxRuntime {
    fn from_env(value: &str) -> anyhow::Result<Self> {
        match value {
            "process" => Ok(Self::Process),
            other => Err(anyhow::anyhow!(
                "Invalid SANDBOX_RUNTIME '{other}'. Expected 'process'"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Port the hypervisor listens on
    pub port: u16,
    /// Path to the sandbox binary
    pub sandbox_binary: String,
    /// Runtime backend for sandbox lifecycle.
    pub sandbox_runtime: SandboxRuntime,
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
    /// Shared token used by sandbox-to-hypervisor provider gateway requests.
    pub provider_gateway_token: Option<String>,
    /// Base URL sandboxes use to reach the hypervisor provider gateway.
    pub provider_gateway_base_url: Option<String>,
    /// Allowed upstream provider base URLs for the gateway.
    pub provider_gateway_allowed_upstreams: Vec<String>,
    /// Per-sandbox request budget over a rolling 60s window.
    pub provider_gateway_rate_limit_per_minute: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let port = env_parse("HYPERVISOR_PORT", 9090)?;
        let provider_gateway_token = std::env::var("CHOIR_PROVIDER_GATEWAY_TOKEN").ok();
        let provider_gateway_base_url = std::env::var("CHOIR_PROVIDER_GATEWAY_BASE_URL")
            .ok()
            .or_else(|| Some(format!("http://127.0.0.1:{port}")));

        Ok(Self {
            port,
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
            sandbox_runtime: SandboxRuntime::from_env(&env_str("SANDBOX_RUNTIME", "process"))?,
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
            provider_gateway_token,
            provider_gateway_base_url,
            provider_gateway_allowed_upstreams: env_csv(
                "CHOIR_PROVIDER_GATEWAY_ALLOWED_UPSTREAMS",
                &[
                    "https://api.z.ai/api/anthropic",
                    "https://api.kimi.com/coding/",
                ],
            ),
            provider_gateway_rate_limit_per_minute: env_parse(
                "CHOIR_PROVIDER_GATEWAY_RATE_LIMIT_PER_MINUTE",
                120,
            )?,
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

fn env_csv(key: &str, default: &[&str]) -> Vec<String> {
    match std::env::var(key) {
        Ok(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect(),
        Err(_) => default.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// Resolve the Dioxus frontend dist directory for unauth auth-page bootstrap.
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
        .join("dioxus-desktop/target/dx/dioxus-desktop/debug/web/public")
        .to_string_lossy()
        .to_string()
}
