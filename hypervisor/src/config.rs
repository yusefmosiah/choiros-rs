use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

// ── Machine class config (ADR-0014 Phase 6) ────────────────────────────────

/// A single machine class: named VM configuration with hypervisor, transport,
/// resource sizing, and resolved nix store paths.
#[derive(Debug, Clone, Deserialize)]
pub struct MachineClass {
    pub hypervisor: String,
    pub transport: String,
    pub vcpu: u32,
    pub memory_mb: u32,
    /// Resolved nix store path to the microvm runner directory.
    pub runner: String,
    /// systemd template prefix (e.g. "cloud-hypervisor" or "firecracker").
    pub systemd_template: String,
}

/// Host-level machine class defaults.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HostConfig {
    pub default_class: Option<String>,
}

/// Top-level machine classes config, parsed from /etc/choiros/machine-classes.toml.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MachineClassesConfig {
    #[serde(default)]
    pub classes: HashMap<String, MachineClass>,
    #[serde(default)]
    pub host: HostConfig,
}

impl MachineClassesConfig {
    /// Load from the TOML file at the given path. Returns empty config if file
    /// doesn't exist (graceful degradation for dev environments).
    pub fn load(path: &str) -> Self {
        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(config) => {
                    tracing::info!(
                        path,
                        classes = config_class_names(&config),
                        default = config.host.default_class.as_deref().unwrap_or("none"),
                        "loaded machine classes config"
                    );
                    config
                }
                Err(e) => {
                    tracing::error!(path, error = %e, "failed to parse machine classes TOML");
                    Self::default()
                }
            },
            Err(_) => {
                tracing::info!(path, "no machine classes config found (dev mode)");
                Self::default()
            }
        }
    }

    /// Look up a class by name, falling back to the host default.
    pub fn resolve(&self, name: Option<&str>) -> Option<&MachineClass> {
        let key = name.or(self.host.default_class.as_deref())?;
        self.classes.get(key)
    }
}

fn config_class_names(config: &MachineClassesConfig) -> String {
    let mut names: Vec<&str> = config.classes.keys().map(|k| k.as_str()).collect();
    names.sort();
    names.join(", ")
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Port the hypervisor listens on
    pub port: u16,
    /// Control command for sandbox runtime lifecycle operations (ensure/stop).
    pub sandbox_runtime_ctl: String,
    /// Port the live sandbox listens on (hypervisor assigns this)
    pub sandbox_live_port: u16,
    /// Port the dev sandbox listens on
    pub sandbox_dev_port: u16,
    /// Inclusive start of dynamic branch runtime port range.
    pub sandbox_branch_port_start: u16,
    /// Inclusive end of dynamic branch runtime port range.
    pub sandbox_branch_port_end: u16,
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
    /// Machine classes config (ADR-0014 Phase 6).
    pub machine_classes: MachineClassesConfig,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let port = env_parse("HYPERVISOR_PORT", 9090)?;
        let provider_gateway_token = std::env::var("CHOIR_PROVIDER_GATEWAY_TOKEN").ok();
        let provider_gateway_base_url = std::env::var("CHOIR_PROVIDER_GATEWAY_BASE_URL")
            .ok()
            .or_else(|| Some(format!("http://127.0.0.1:{port}")));
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let default_runtime_ctl = workspace_root
            .join("target/debug/vfkit-runtime-ctl")
            .to_string_lossy()
            .to_string();

        // Accept both SANDBOX_RUNTIME_CTL (new) and SANDBOX_VFKIT_CTL (legacy)
        let runtime_ctl = std::env::var("SANDBOX_RUNTIME_CTL")
            .or_else(|_| std::env::var("SANDBOX_VFKIT_CTL"))
            .unwrap_or(default_runtime_ctl);

        let cfg = Self {
            port,
            sandbox_runtime_ctl: runtime_ctl,
            sandbox_live_port: env_parse("SANDBOX_LIVE_PORT", 8080)?,
            sandbox_dev_port: env_parse("SANDBOX_DEV_PORT", 8081)?,
            sandbox_branch_port_start: env_parse("SANDBOX_BRANCH_PORT_START", 12000)?,
            sandbox_branch_port_end: env_parse("SANDBOX_BRANCH_PORT_END", 12999)?,
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
                    "https://api.openai.com",
                    "https://api.inceptionlabs.ai/v1",
                    "https://openrouter.ai/api/v1",
                    "https://api.tavily.com",
                    "https://api.search.brave.com",
                    "https://api.exa.ai",
                ],
            ),
            provider_gateway_rate_limit_per_minute: env_parse(
                "CHOIR_PROVIDER_GATEWAY_RATE_LIMIT_PER_MINUTE",
                120,
            )?,
            machine_classes: MachineClassesConfig::load(&env_str(
                "CHOIR_MACHINE_CLASSES_PATH",
                "/etc/choiros/machine-classes.toml",
            )),
        };

        if cfg.sandbox_branch_port_start > cfg.sandbox_branch_port_end {
            return Err(anyhow::anyhow!(
                "Invalid branch port range: SANDBOX_BRANCH_PORT_START ({}) > SANDBOX_BRANCH_PORT_END ({})",
                cfg.sandbox_branch_port_start,
                cfg.sandbox_branch_port_end
            ));
        }

        if cfg.sandbox_runtime_ctl.trim().is_empty() {
            return Err(anyhow::anyhow!(
                "SANDBOX_RUNTIME_CTL (or SANDBOX_VFKIT_CTL) must be set to a runtime control command"
            ));
        }

        Ok(cfg)
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

    // Keep frontend dist resolution in lockstep with sandbox runtime so auth
    // shell HTML and proxied static asset routes always agree.
    let release = workspace_root.join("dioxus-desktop/target/dx/dioxus-desktop/release/web/public");
    if release.join("index.html").exists() {
        return release.to_string_lossy().to_string();
    }

    let debug = workspace_root.join("dioxus-desktop/target/dx/dioxus-desktop/debug/web/public");
    if debug.join("index.html").exists() {
        return debug.to_string_lossy().to_string();
    }

    // Final fallback for environments that provide dist through runtime wiring.
    debug.to_string_lossy().to_string()
}
