use std::collections::HashSet;
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use sha1::{Digest, Sha1};

const SSH_SERVER_ALIVE_INTERVAL_SECS: u64 = 10;
const SSH_SERVER_ALIVE_COUNT_MAX: u64 = 3;
const SSH_CONNECT_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Ensure,
    Stop,
}

impl Action {
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "ensure" => Ok(Self::Ensure),
            "stop" => Ok(Self::Stop),
            other => bail!("invalid action '{other}' (expected ensure|stop)"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Ensure => "ensure",
            Self::Stop => "stop",
        }
    }
}

#[derive(Debug, Clone)]
struct CliArgs {
    action: Action,
    user_id: String,
    runtime: String,
    port: u16,
    role: Option<String>,
    branch: Option<String>,
}

impl CliArgs {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);

        let Some(action_raw) = args.next() else {
            bail!(
                "usage: vfkit-runtime-ctl <ensure|stop> --user-id <id> --runtime <name> --port <port> [--role <live|dev>] [--branch <branch>]"
            );
        };
        let action = Action::parse(&action_raw)?;

        let mut user_id = None;
        let mut runtime = None;
        let mut port = None;
        let mut role = None;
        let mut branch = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--user-id" => user_id = args.next(),
                "--runtime" => runtime = args.next(),
                "--port" => {
                    let Some(raw) = args.next() else {
                        bail!("missing value for --port");
                    };
                    let parsed = raw
                        .parse::<u16>()
                        .with_context(|| format!("invalid --port '{raw}'"))?;
                    port = Some(parsed);
                }
                "--role" => role = args.next(),
                "--branch" => branch = args.next(),
                other => bail!("unknown arg: {other}"),
            }
        }

        let Some(user_id) = user_id else {
            bail!("missing required args; need --user-id, --runtime, --port");
        };
        let Some(runtime) = runtime else {
            bail!("missing required args; need --user-id, --runtime, --port");
        };
        let Some(port) = port else {
            bail!("missing required args; need --user-id, --runtime, --port");
        };

        Ok(Self {
            action,
            user_id,
            runtime,
            port,
            role,
            branch,
        })
    }
}

#[derive(Debug, Clone)]
struct Endpoint {
    host: String,
    port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AllocatePty {
    Auto,
    True,
    False,
}

impl AllocatePty {
    fn parse(raw: &str) -> Result<Self> {
        match raw {
            "auto" => Ok(Self::Auto),
            "true" => Ok(Self::True),
            "false" => Ok(Self::False),
            other => bail!("invalid CHOIR_VFKIT_ALLOCATE_PTY={other} (expected auto|true|false)"),
        }
    }
}

#[derive(Debug, Clone)]
struct Config {
    action: Action,
    user_id: String,
    runtime: String,
    port: u16,
    role: Option<String>,
    branch: Option<String>,

    root_dir: PathBuf,
    runtime_state_dir: PathBuf,
    guest_state_root: PathBuf,

    vm_pid_file: PathBuf,
    vm_log_file: PathBuf,
    vm_work_dir: PathBuf,
    vm_socket_path: PathBuf,

    runtime_pid_file: PathBuf,
    runtime_log_file: PathBuf,

    ssh_key_file: PathBuf,
    ssh_port: u16,

    vm_runner_cmd: String,
    nix_config_merged: String,

    guest_host: String,
    guest_user: String,
    guest_ctl: String,
    guest_name: String,
    guest_mac_addr: String,
    guest_port_override: Option<u16>,
    allow_dhcp_lease_fallback: bool,

    host_is_darwin: bool,
    manage_vm: bool,
    auto_stop_vm: bool,
    skip_ssh_wait: bool,
    ssh_wait_secs: u64,
    allocate_pty: AllocatePty,

    ensure_override_cmd: Option<String>,
    stop_override_cmd: Option<String>,
}

impl Config {
    fn from_cli(cli: CliArgs) -> Result<Self> {
        let root_dir = env::var("CHOIR_WORKSPACE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."))
            });

        let state_dir = env::var("CHOIR_VFKIT_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home_dir().join(".local/share/choiros/vfkit"));

        let vm_state_dir = state_dir.join("vms");
        let runtime_state_dir = state_dir.join("runtimes");
        let guest_state_root = env::var("CHOIR_VFKIT_GUEST_STATE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| state_dir.join("guest"));
        let keys_dir = state_dir.join("keys");

        fs::create_dir_all(&vm_state_dir)
            .with_context(|| format!("failed to create {}", vm_state_dir.display()))?;
        fs::create_dir_all(&runtime_state_dir)
            .with_context(|| format!("failed to create {}", runtime_state_dir.display()))?;
        fs::create_dir_all(&keys_dir)
            .with_context(|| format!("failed to create {}", keys_dir.display()))?;
        fs::create_dir_all(&guest_state_root)
            .with_context(|| format!("failed to create {}", guest_state_root.display()))?;

        let store_overlay_host_dir = guest_state_root.join("store-overlay");
        fs::create_dir_all(store_overlay_host_dir.join("store")).with_context(|| {
            format!(
                "failed to create {}",
                store_overlay_host_dir.join("store").display()
            )
        })?;
        fs::create_dir_all(store_overlay_host_dir.join("work")).with_context(|| {
            format!(
                "failed to create {}",
                store_overlay_host_dir.join("work").display()
            )
        })?;

        let user_slug = slugify(&cli.user_id);
        let user_hash10 = hash_prefix(&cli.user_id, 10);
        let runtime_slug = slugify(&format!("{}-{}", cli.user_id, cli.runtime));

        let vm_pid_file = vm_state_dir.join(format!("{user_slug}.pid"));
        let vm_log_file = vm_state_dir.join(format!("{user_slug}.log"));
        let vm_work_dir = vm_state_dir.join(format!("u-{user_hash10}"));

        let runtime_pid_file = runtime_state_dir.join(format!("{runtime_slug}.pid"));
        let runtime_log_file = runtime_state_dir.join(format!("{runtime_slug}.log"));

        let ssh_key_file = env::var("CHOIR_VFKIT_SSH_KEY_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| keys_dir.join("runtime_ed25519"));

        let ssh_base = parse_env_u16("CHOIR_VFKIT_SSH_PORT_BASE", 22000)?;
        let user_hash_hex = hash_short(&cli.user_id);
        let user_hash_dec = u32::from_str_radix(&user_hash_hex, 16)
            .with_context(|| format!("invalid user hash hex: {user_hash_hex}"))?;
        let ssh_port = ssh_base + ((user_hash_dec % 1000) as u16);

        let vm_runner_cmd = env::var("CHOIR_VFKIT_VM_RUNNER_CMD").unwrap_or_else(|_| {
            format!(
                "nix run --impure \"path:{}#nixosConfigurations.choiros-vfkit-user.config.microvm.runner.vfkit\"",
                root_dir.display()
            )
        });

        let host_is_darwin = cfg!(target_os = "macos");
        let default_nix_config_override = if host_is_darwin {
            "extra-platforms = x86_64-darwin".to_string()
        } else {
            String::new()
        };
        let nix_config_override =
            env::var("CHOIR_VFKIT_NIX_CONFIG_OVERRIDE").unwrap_or(default_nix_config_override);

        let mut nix_config_merged = env::var("NIX_CONFIG").unwrap_or_default();
        if !nix_config_override.trim().is_empty() {
            if !nix_config_merged.trim().is_empty() {
                nix_config_merged.push('\n');
            }
            nix_config_merged.push_str(&nix_config_override);
        }

        let guest_host = env::var("CHOIR_VFKIT_GUEST_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let guest_user = env::var("CHOIR_VFKIT_GUEST_USER").unwrap_or_else(|_| "root".into());
        let guest_ctl = env::var("CHOIR_VFKIT_GUEST_CTL")
            .unwrap_or_else(|_| "choir-vfkit-guest-runtime-ctl".into());
        let default_guest_name = format!("cvm-{user_hash10}");
        let guest_name = env::var("CHOIR_VFKIT_GUEST_NAME").unwrap_or(default_guest_name);
        let guest_mac_addr = env::var("CHOIR_VFKIT_MAC_ADDR")
            .unwrap_or_else(|_| derive_guest_mac_addr(&cli.user_id));
        let guest_port_override = parse_env_optional_u16("CHOIR_VFKIT_GUEST_PORT")?;
        let allow_dhcp_lease_fallback =
            parse_env_bool("CHOIR_VFKIT_ALLOW_DHCP_LEASE_FALLBACK", true);

        let vm_socket_path = vm_work_dir.join(format!("{guest_name}.sock"));

        let manage_vm = parse_env_bool("CHOIR_VFKIT_MANAGE_VM", true);
        let auto_stop_vm = parse_env_bool("CHOIR_VFKIT_AUTO_STOP_VM", true);
        let skip_ssh_wait = parse_env_bool("CHOIR_VFKIT_SKIP_SSH_WAIT", false);
        let ssh_wait_secs = parse_env_u64("CHOIR_VFKIT_SSH_WAIT_SECS", 420)?;

        let allocate_pty = AllocatePty::parse(
            &env::var("CHOIR_VFKIT_ALLOCATE_PTY").unwrap_or_else(|_| "auto".to_string()),
        )?;

        let ensure_override_cmd = env_non_empty("CHOIR_VFKIT_ENSURE_CMD");
        let stop_override_cmd = env_non_empty("CHOIR_VFKIT_STOP_CMD");

        Ok(Self {
            action: cli.action,
            user_id: cli.user_id,
            runtime: cli.runtime,
            port: cli.port,
            role: cli.role,
            branch: cli.branch,

            root_dir,
            runtime_state_dir,
            guest_state_root,

            vm_pid_file,
            vm_log_file,
            vm_work_dir,
            vm_socket_path,

            runtime_pid_file,
            runtime_log_file,

            ssh_key_file,
            ssh_port,

            vm_runner_cmd,
            nix_config_merged,

            guest_host,
            guest_user,
            guest_ctl,
            guest_name,
            guest_mac_addr,
            guest_port_override,
            allow_dhcp_lease_fallback,

            host_is_darwin,
            manage_vm,
            auto_stop_vm,
            skip_ssh_wait,
            ssh_wait_secs,
            allocate_pty,

            ensure_override_cmd,
            stop_override_cmd,
        })
    }
}

struct RuntimeCtl {
    cfg: Config,
    resolved_guest_endpoint: Option<Endpoint>,
}

impl RuntimeCtl {
    fn new(cfg: Config) -> Self {
        Self {
            cfg,
            resolved_guest_endpoint: None,
        }
    }

    fn run(mut self) -> Result<()> {
        match self.cfg.action {
            Action::Ensure => {
                if let Some(cmd) = self.cfg.ensure_override_cmd.clone() {
                    return run_external(&cmd);
                }
                self.ensure_vm_running()?;
                self.run_guest_ctl()?;
                self.ensure_tunnel()?;
            }
            Action::Stop => {
                if let Some(cmd) = self.cfg.stop_override_cmd.clone() {
                    return run_external(&cmd);
                }
                if let Err(err) = self.run_guest_ctl() {
                    eprintln!("WARN guest stop request failed: {err:#}");
                }
                self.stop_tunnel()?;
                self.maybe_stop_vm()?;
            }
        }
        Ok(())
    }

    fn ensure_ssh_identity(&self) -> Result<()> {
        let pubkey = ssh_pubkey_path(&self.cfg.ssh_key_file);
        if self.cfg.ssh_key_file.exists() && pubkey.exists() {
            return Ok(());
        }

        if env::var("CHOIR_VFKIT_SSH_KEY_PATH").is_ok() {
            bail!(
                "missing SSH keypair at {} (and {})",
                self.cfg.ssh_key_file.display(),
                pubkey.display()
            );
        }

        if !command_exists("ssh-keygen") {
            bail!("ssh-keygen is required to provision vfkit automation SSH keys");
        }

        if let Some(parent) = self.cfg.ssh_key_file.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let status = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&self.cfg.ssh_key_file)
            .status()
            .context("failed to run ssh-keygen")?;

        if !status.success() {
            bail!("ssh-keygen failed");
        }

        Ok(())
    }

    fn resolve_ssh_pubkey(&self) -> Result<String> {
        self.ensure_ssh_identity()?;
        let pubkey = ssh_pubkey_path(&self.cfg.ssh_key_file);
        let content = fs::read_to_string(&pubkey)
            .with_context(|| format!("failed to read {}", pubkey.display()))?;
        let key = content.trim().to_string();
        if key.is_empty() {
            bail!("SSH public key is empty: {}", pubkey.display());
        }
        Ok(key)
    }

    fn resolve_guest_ips_from_leases(&self) -> Vec<(String, String)> {
        let leases_path = Path::new("/var/db/dhcpd_leases");
        let Ok(content) = fs::read_to_string(leases_path) else {
            return Vec::new();
        };

        let mut leases: Vec<(String, String)> = Vec::new();

        let mut in_lease = false;
        let mut lease_name = String::new();
        let mut lease_ip = String::new();
        let mut lease_hex = String::new();

        for raw in content.lines() {
            let line = raw.trim();
            if line == "{" {
                in_lease = true;
                lease_name.clear();
                lease_ip.clear();
                lease_hex.clear();
                continue;
            }

            if in_lease && line == "}" {
                if lease_name == self.cfg.guest_name
                    && !lease_ip.is_empty()
                    && !lease_hex.is_empty()
                {
                    leases.push((lease_hex.clone(), lease_ip.clone()));
                }
                in_lease = false;
                continue;
            }

            if !in_lease {
                continue;
            }

            if let Some(rest) = line.strip_prefix("name=") {
                lease_name = rest.trim().to_string();
                continue;
            }
            if let Some(rest) = line.strip_prefix("ip_address=") {
                lease_ip = rest.trim().to_string();
                continue;
            }
            if let Some(rest) = line.strip_prefix("lease=0x") {
                lease_hex = rest.trim().to_string();
            }
        }

        leases.sort_by(|a, b| b.0.cmp(&a.0));
        leases
    }

    fn resolve_guest_endpoints(&self) -> Vec<Endpoint> {
        let mut endpoints = Vec::new();

        if !self.cfg.guest_host.is_empty() && self.cfg.guest_host != "127.0.0.1" {
            let port = self.cfg.guest_port_override.unwrap_or(22);
            endpoints.push(Endpoint {
                host: self.cfg.guest_host.clone(),
                port,
            });
            return endpoints;
        }

        // Prefer deterministic per-user local forwarded SSH port first.
        // Lease scanning can pick the wrong VM when multiple guests share the
        // same hostname in the DHCP table.
        endpoints.push(Endpoint {
            host: "127.0.0.1".to_string(),
            port: self.cfg.guest_port_override.unwrap_or(self.cfg.ssh_port),
        });

        if self.cfg.host_is_darwin && self.cfg.allow_dhcp_lease_fallback {
            for (_, ip) in self.resolve_guest_ips_from_leases() {
                endpoints.push(Endpoint { host: ip, port: 22 });
            }
        }

        let mut seen = HashSet::new();
        endpoints
            .into_iter()
            .filter(|endpoint| seen.insert(format!("{}:{}", endpoint.host, endpoint.port)))
            .collect()
    }

    fn ssh_args_for_endpoint(&self, endpoint: &Endpoint) -> Vec<String> {
        vec![
            "-p".to_string(),
            endpoint.port.to_string(),
            "-o".to_string(),
            "StrictHostKeyChecking=no".to_string(),
            "-o".to_string(),
            "UserKnownHostsFile=/dev/null".to_string(),
            "-o".to_string(),
            "LogLevel=ERROR".to_string(),
            "-o".to_string(),
            "BatchMode=yes".to_string(),
            "-o".to_string(),
            "IdentitiesOnly=yes".to_string(),
            "-o".to_string(),
            format!("ServerAliveInterval={SSH_SERVER_ALIVE_INTERVAL_SECS}"),
            "-o".to_string(),
            format!("ServerAliveCountMax={SSH_SERVER_ALIVE_COUNT_MAX}"),
            "-o".to_string(),
            format!("ConnectTimeout={SSH_CONNECT_TIMEOUT_SECS}"),
            "-i".to_string(),
            self.cfg.ssh_key_file.display().to_string(),
            format!("{}@{}", self.cfg.guest_user, endpoint.host),
        ]
    }

    fn ssh_ok(&self, endpoint: &Endpoint) -> bool {
        let mut cmd = Command::new("ssh");
        cmd.args(self.ssh_args_for_endpoint(endpoint));
        cmd.arg("true");
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        cmd.status().map(|s| s.success()).unwrap_or(false)
    }

    fn wait_for_ssh(&mut self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(self.cfg.ssh_wait_secs);

        while Instant::now() < deadline {
            for endpoint in self.resolve_guest_endpoints() {
                if self.ssh_ok(&endpoint) {
                    self.resolved_guest_endpoint = Some(endpoint);
                    return true;
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }

        false
    }

    fn ensure_guest_endpoint(&mut self) -> Result<Endpoint> {
        if let Some(endpoint) = self.resolved_guest_endpoint.clone() {
            if self.ssh_ok(&endpoint) {
                return Ok(endpoint);
            }
            self.resolved_guest_endpoint = None;
        }

        if self.wait_for_ssh() {
            if let Some(endpoint) = self.resolved_guest_endpoint.clone() {
                return Ok(endpoint);
            }
        }

        let endpoints = self
            .resolve_guest_endpoints()
            .into_iter()
            .map(|ep| format!("{}:{}", ep.host, ep.port))
            .collect::<Vec<_>>()
            .join(", ");

        bail!(
            "vfkit guest SSH did not become reachable (tried: {endpoints})\nvm log: {}",
            self.cfg.vm_log_file.display()
        )
    }

    fn ensure_vm_running(&mut self) -> Result<()> {
        if !self.cfg.manage_vm {
            return Ok(());
        }

        self.ensure_ssh_identity()?;
        let ssh_pubkey = self.resolve_ssh_pubkey()?;

        if let Some(pid) = read_pid(&self.cfg.vm_pid_file) {
            if is_pid_alive(pid) {
                return Ok(());
            }
            remove_file_if_exists(&self.cfg.vm_pid_file)?;
        }

        if let Some(pid) = self.find_vm_pid()? {
            if is_pid_alive(pid) {
                write_pid(&self.cfg.vm_pid_file, pid)?;
                return Ok(());
            }
        }

        fs::create_dir_all(&self.cfg.vm_work_dir)
            .with_context(|| format!("failed to create {}", self.cfg.vm_work_dir.display()))?;

        let use_pty = match self.cfg.allocate_pty {
            AllocatePty::True => true,
            AllocatePty::False => false,
            AllocatePty::Auto => self.cfg.host_is_darwin,
        };

        let run_body = format!(
            "cd '{}' && {}",
            shell_quote(self.cfg.vm_work_dir.display().to_string()),
            self.cfg.vm_runner_cmd
        );

        let mut cmd = if use_pty && command_exists("script") {
            let mut c = Command::new("script");
            c.args(["-q", "/dev/null", "/bin/bash", "-lc", &run_body]);
            c
        } else {
            let mut c = Command::new("/bin/bash");
            c.args(["-lc", &run_body]);
            c
        };

        cmd.env("CHOIR_VFKIT_USER_ID", &self.cfg.user_id)
            .env("CHOIR_VFKIT_SSH_PORT", self.cfg.ssh_port.to_string())
            .env("CHOIR_VFKIT_SSH_PUBKEY", ssh_pubkey)
            .env("CHOIR_VFKIT_GUEST_NAME", &self.cfg.guest_name)
            .env("CHOIR_VFKIT_MAC_ADDR", &self.cfg.guest_mac_addr)
            .env(
                "CHOIR_VFKIT_GUEST_STATE_ROOT",
                self.cfg.guest_state_root.display().to_string(),
            )
            .env(
                "CHOIR_WORKSPACE_ROOT",
                self.cfg.root_dir.display().to_string(),
            )
            .env("NIX_CONFIG", &self.cfg.nix_config_merged);

        let log_file = File::create(&self.cfg.vm_log_file)
            .with_context(|| format!("failed to open {}", self.cfg.vm_log_file.display()))?;
        let log_file_err = log_file
            .try_clone()
            .context("failed to clone VM log file handle")?;

        cmd.stdout(Stdio::from(log_file));
        cmd.stderr(Stdio::from(log_file_err));

        let child = cmd.spawn().context("failed to launch vfkit vm runner")?;
        write_pid(&self.cfg.vm_pid_file, child.id() as i32)?;

        if self.cfg.skip_ssh_wait {
            return Ok(());
        }

        if !self.wait_for_ssh() {
            let endpoints = self
                .resolve_guest_endpoints()
                .into_iter()
                .map(|ep| format!("{}:{}", ep.host, ep.port))
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "vfkit guest SSH did not become reachable (tried: {endpoints})\nvm log: {}",
                self.cfg.vm_log_file.display()
            );
        }

        if let Some(pid) = self.find_vm_pid()? {
            if is_pid_alive(pid) {
                write_pid(&self.cfg.vm_pid_file, pid)?;
            }
        }

        Ok(())
    }

    fn find_vm_pid(&self) -> Result<Option<i32>> {
        let lines = ps_command_lines()?;
        let sock = self.cfg.vm_socket_path.display().to_string();

        for line in lines {
            if line.contains("/bin/vfkit") && line.contains(&sock) {
                if let Some(pid) = parse_pid_from_ps_line(&line) {
                    return Ok(Some(pid));
                }
            }
        }

        Ok(None)
    }

    fn find_vm_launcher_pids(&self) -> Result<Vec<i32>> {
        let lines = ps_command_lines()?;
        let mut pids = Vec::new();
        let workdir = self.cfg.vm_work_dir.display().to_string();

        for line in lines {
            if line.contains("bash") && line.contains(&workdir) && line.contains("nix run --impure")
            {
                if let Some(pid) = parse_pid_from_ps_line(&line) {
                    pids.push(pid);
                }
            }
        }

        Ok(pids)
    }

    fn run_guest_ctl(&mut self) -> Result<()> {
        let endpoint = self.ensure_guest_endpoint()?;

        let mut cmd = Command::new("ssh");
        cmd.args(self.ssh_args_for_endpoint(&endpoint));
        cmd.arg(&self.cfg.guest_ctl)
            .arg(self.cfg.action.as_str())
            .arg("--runtime")
            .arg(&self.cfg.runtime)
            .arg("--port")
            .arg(self.cfg.port.to_string());

        if let Some(role) = self.cfg.role.as_ref() {
            cmd.arg("--role").arg(role);
        }
        if let Some(branch) = self.cfg.branch.as_ref() {
            cmd.arg("--branch").arg(branch);
        }

        let output = cmd
            .output()
            .context("failed to run guest runtime ctl over ssh")?;
        if output.status.success() {
            return Ok(());
        }

        bail!(
            "guest runtime ctl failed: code={:?} stdout='{}' stderr='{}'",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }

    fn ensure_tunnel(&mut self) -> Result<()> {
        let endpoint = self.ensure_guest_endpoint()?;

        if let Some(pid) = read_pid(&self.cfg.runtime_pid_file) {
            if is_pid_alive(pid) {
                return Ok(());
            }
            remove_file_if_exists(&self.cfg.runtime_pid_file)?;
        }

        if let Some(existing_listener_pid) = first_listening_pid_on_port(self.cfg.port) {
            if is_pid_alive(existing_listener_pid) {
                let existing_cmd = ps_command_for_pid(existing_listener_pid).unwrap_or_default();
                let expected_fwd =
                    format!("127.0.0.1:{}:127.0.0.1:{}", self.cfg.port, self.cfg.port);
                if existing_cmd.contains("ssh") && existing_cmd.contains(&expected_fwd) {
                    kill_pid(existing_listener_pid);
                    std::thread::sleep(Duration::from_secs(1));
                } else {
                    bail!(
                        "local port {} is already in use by pid {}\nconflicting command: {}",
                        self.cfg.port,
                        existing_listener_pid,
                        if existing_cmd.is_empty() {
                            "unknown"
                        } else {
                            &existing_cmd
                        }
                    );
                }
            }
        }

        let mut runtime_log = File::create(&self.cfg.runtime_log_file)
            .with_context(|| format!("failed to open {}", self.cfg.runtime_log_file.display()))?;
        runtime_log
            .write_all(b"")
            .context("failed to initialize runtime tunnel log")?;

        let mut tunnel_cmd = Command::new("ssh");
        tunnel_cmd
            .arg("-p")
            .arg(endpoint.port.to_string())
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "LogLevel=ERROR",
                "-o",
                "BatchMode=yes",
                "-o",
                "IdentitiesOnly=yes",
                "-o",
                &format!("ServerAliveInterval={SSH_SERVER_ALIVE_INTERVAL_SECS}"),
                "-o",
                &format!("ServerAliveCountMax={SSH_SERVER_ALIVE_COUNT_MAX}"),
                "-o",
                &format!("ConnectTimeout={SSH_CONNECT_TIMEOUT_SECS}"),
                "-i",
            ])
            .arg(&self.cfg.ssh_key_file)
            .args(["-f", "-n", "-N", "-L"])
            .arg(format!(
                "127.0.0.1:{}:127.0.0.1:{}",
                self.cfg.port, self.cfg.port
            ))
            .arg(format!("{}@{}", self.cfg.guest_user, endpoint.host));

        let runtime_log_append = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.cfg.runtime_log_file)
            .with_context(|| format!("failed to open {}", self.cfg.runtime_log_file.display()))?;
        let runtime_log_append_err = runtime_log_append
            .try_clone()
            .context("failed to clone runtime tunnel log file")?;

        tunnel_cmd.stdout(Stdio::from(runtime_log_append));
        tunnel_cmd.stderr(Stdio::from(runtime_log_append_err));

        let status = tunnel_cmd
            .status()
            .context("failed to start SSH tunnel process")?;
        if !status.success() {
            bail!(
                "failed to start ssh tunnel for runtime {}\ntunnel log: {}",
                self.cfg.runtime,
                self.cfg.runtime_log_file.display()
            );
        }

        let expected_fwd = format!("127.0.0.1:{}:127.0.0.1:{}", self.cfg.port, self.cfg.port);

        let mut tunnel_pid = self
            .find_matching_tunnel_pids()
            .ok()
            .and_then(|pids| pids.into_iter().find(|pid| is_pid_alive(*pid)));

        if tunnel_pid.is_none() {
            if let Some(listener_pid) = first_listening_pid_on_port(self.cfg.port) {
                let listener_cmd = ps_command_for_pid(listener_pid).unwrap_or_default();
                if listener_cmd.contains("ssh")
                    && listener_cmd.contains(&expected_fwd)
                    && is_pid_alive(listener_pid)
                {
                    tunnel_pid = Some(listener_pid);
                }
            }
        }

        let Some(pid) = tunnel_pid else {
            bail!(
                "ssh tunnel did not stay up for runtime {} on local port {}\ntunnel log: {}",
                self.cfg.runtime,
                self.cfg.port,
                self.cfg.runtime_log_file.display()
            );
        };

        // Guard against short-lived ssh -f backgrounds that exit right after
        // daemonizing due to connect/auth failures.
        std::thread::sleep(Duration::from_secs(1));
        if !is_pid_alive(pid) {
            bail!(
                "ssh tunnel did not stay up for runtime {} on local port {}\ntunnel log: {}",
                self.cfg.runtime,
                self.cfg.port,
                self.cfg.runtime_log_file.display()
            );
        }

        write_pid(&self.cfg.runtime_pid_file, pid)?;
        Ok(())
    }

    fn find_matching_tunnel_pids(&self) -> Result<Vec<i32>> {
        let mut pids = Vec::new();
        let forward = format!("127.0.0.1:{}:127.0.0.1:{}", self.cfg.port, self.cfg.port);

        for line in ps_command_lines()? {
            if line.contains("ssh") && line.contains(&forward) {
                if let Some(pid) = parse_pid_from_ps_line(&line) {
                    pids.push(pid);
                }
            }
        }

        Ok(pids)
    }

    fn stop_tunnel(&self) -> Result<()> {
        if let Some(pid) = read_pid(&self.cfg.runtime_pid_file) {
            if is_pid_alive(pid) {
                kill_pid(pid);
            }
            remove_file_if_exists(&self.cfg.runtime_pid_file)?;
        }

        for pid in self.find_matching_tunnel_pids()? {
            if is_pid_alive(pid) {
                kill_pid(pid);
            }
        }

        Ok(())
    }

    fn maybe_stop_vm(&self) -> Result<()> {
        if !self.cfg.manage_vm || !self.cfg.auto_stop_vm {
            return Ok(());
        }

        if runtime_pid_exists(&self.cfg.runtime_state_dir)? {
            return Ok(());
        }

        if let Some(pid) = read_pid(&self.cfg.vm_pid_file) {
            if is_pid_alive(pid) {
                kill_pid(pid);
            }
        }

        while let Some(pid) = self.find_vm_pid()? {
            if !is_pid_alive(pid) {
                break;
            }
            kill_pid(pid);
            std::thread::sleep(Duration::from_secs(1));
        }

        for pid in self.find_vm_launcher_pids()? {
            if is_pid_alive(pid) {
                kill_pid(pid);
            }
        }

        remove_file_if_exists(&self.cfg.vm_pid_file)?;

        Ok(())
    }
}

fn shell_quote(input: String) -> String {
    input.replace('\'', "'\\''")
}

fn slugify(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn hash_short(input: &str) -> String {
    hash_prefix(input, 6)
}

fn hash_prefix(input: &str, len: usize) -> String {
    let mut hasher = Sha1::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    let encoded = hex::encode(digest);
    let keep = len.min(encoded.len());
    encoded[0..keep].to_string()
}

fn derive_guest_mac_addr(user_id: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(user_id.as_bytes());
    let digest = hasher.finalize();
    format!(
        "56:46:4b:{:02x}:{:02x}:{:02x}",
        digest[0], digest[1], digest[2]
    )
}

fn parse_env_u16(key: &str, default: u16) -> Result<u16> {
    match env::var(key) {
        Ok(raw) => raw
            .parse::<u16>()
            .with_context(|| format!("failed to parse {key}={raw} as u16")),
        Err(_) => Ok(default),
    }
}

fn parse_env_optional_u16(key: &str) -> Result<Option<u16>> {
    match env::var(key) {
        Ok(raw) if raw.trim().is_empty() => Ok(None),
        Ok(raw) => raw
            .parse::<u16>()
            .map(Some)
            .with_context(|| format!("failed to parse {key}={raw} as u16")),
        Err(_) => Ok(None),
    }
}

fn parse_env_u64(key: &str, default: u64) -> Result<u64> {
    match env::var(key) {
        Ok(raw) => raw
            .parse::<u64>()
            .with_context(|| format!("failed to parse {key}={raw} as u64")),
        Err(_) => Ok(default),
    }
}

fn parse_env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

fn env_non_empty(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .and_then(|v| if v.trim().is_empty() { None } else { Some(v) })
}

fn home_dir() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn ssh_pubkey_path(private_key: &Path) -> PathBuf {
    PathBuf::from(format!("{}.pub", private_key.display()))
}

fn command_exists(command: &str) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };

    env::split_paths(&path_var)
        .map(|p| p.join(command))
        .any(|p| p.exists())
}

fn parse_pid_from_ps_line(line: &str) -> Option<i32> {
    line.split_whitespace().next()?.parse::<i32>().ok()
}

fn ps_command_lines() -> Result<Vec<String>> {
    let output = Command::new("ps")
        .args(["ax", "-o", "pid=", "-o", "command="])
        .output()
        .context("failed to run ps")?;

    if !output.status.success() {
        bail!(
            "ps command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

fn ps_command_for_pid(pid: i32) -> Option<String> {
    let output = Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .args(["-o", "command="])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let cmd = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if cmd.is_empty() {
        None
    } else {
        Some(cmd)
    }
}

fn is_pid_alive(pid: i32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn kill_pid(pid: i32) {
    let _ = Command::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn read_pid(path: &Path) -> Option<i32> {
    let raw = fs::read_to_string(path).ok()?;
    raw.trim().parse::<i32>().ok()
}

fn write_pid(path: &Path, pid: i32) -> Result<()> {
    fs::write(path, format!("{pid}\n"))
        .with_context(|| format!("failed to write pid file {}", path.display()))
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn runtime_pid_exists(runtime_state_dir: &Path) -> Result<bool> {
    let entries = match fs::read_dir(runtime_state_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read {}", runtime_state_dir.display()));
        }
    };

    for entry in entries {
        let entry = entry
            .with_context(|| format!("failed to read entry in {}", runtime_state_dir.display()))?;
        if entry.path().extension().and_then(|s| s.to_str()) == Some("pid") {
            return Ok(true);
        }
    }

    Ok(false)
}

fn first_listening_pid_on_port(port: u16) -> Option<i32> {
    if !command_exists("lsof") {
        return None;
    }

    let output = Command::new("lsof")
        .args(["-nP", "-tiTCP"])
        .arg(port.to_string())
        .args(["-sTCP:LISTEN"])
        .output()
        .ok()?;

    if !output.status.success() && output.stdout.is_empty() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.trim().parse::<i32>().ok())
}

fn run_external(cmd: &str) -> Result<()> {
    let status = Command::new("/bin/bash")
        .arg("-lc")
        .arg(cmd)
        .status()
        .with_context(|| format!("failed to execute override command: {cmd}"))?;

    if !status.success() {
        bail!("override command failed (code={:?}): {cmd}", status.code());
    }

    Ok(())
}

fn main() {
    if let Err(err) = run_main() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run_main() -> Result<()> {
    let cli = CliArgs::parse()?;
    let cfg = Config::from_cli(cli)?;
    RuntimeCtl::new(cfg).run()
}
