use sandbox::self_directed_dispatch::{
    claim_next_ready_work, preview_next_ready_work, DispatchError, ReadyWorkItem,
    DEFAULT_READY_LIMIT, SELECTION_RULE,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug)]
struct Config {
    repo: PathBuf,
    claimant: String,
    lease: String,
    limit: usize,
    dry_run: bool,
}

#[derive(Debug, Serialize)]
struct BootstrapResponse {
    repo: String,
    mode: &'static str,
    selection_rule: &'static str,
    work: ReadyWorkItem,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    repo: Option<String>,
    error: String,
}

const DEFAULT_CLAIMANT: &str = "repo-worker-bootstrap";
const DEFAULT_LEASE: &str = "15m";

fn usage() -> &'static str {
    "Usage: repo-worker-bootstrap --repo <path> [--claimant <name>] [--lease <duration>] [--limit <n>] [--dry-run]"
}

fn parse_args() -> Result<Config, String> {
    parse_args_from(std::env::args().skip(1))
}

fn parse_args_from<I>(args: I) -> Result<Config, String>
where
    I: IntoIterator<Item = String>,
{
    let mut repo = None;
    let mut claimant = DEFAULT_CLAIMANT.to_string();
    let mut lease = DEFAULT_LEASE.to_string();
    let mut limit = DEFAULT_READY_LIMIT;
    let mut dry_run = false;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--repo requires a path".to_string())?;
                repo = Some(PathBuf::from(value));
            }
            "--claimant" => {
                claimant = args
                    .next()
                    .ok_or_else(|| "--claimant requires a value".to_string())?;
            }
            "--lease" => {
                lease = args
                    .next()
                    .ok_or_else(|| "--lease requires a value".to_string())?;
            }
            "--limit" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--limit requires a value".to_string())?;
                limit = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value: {value}"))?;
            }
            "--dry-run" => dry_run = true,
            "--help" | "-h" => return Err(usage().to_string()),
            other => return Err(format!("unknown argument: {other}\n{}", usage())),
        }
    }

    let repo = repo.ok_or_else(|| format!("missing required --repo\n{}", usage()))?;
    Ok(Config {
        repo,
        claimant,
        lease,
        limit,
        dry_run,
    })
}

fn mode_for_config(config: &Config) -> &'static str {
    if config.dry_run {
        "dry_run"
    } else {
        "claim"
    }
}

fn error_exit_code(error: &DispatchError) -> i32 {
    match error {
        DispatchError::NoReadyWork | DispatchError::NoClaimableWork => 3,
        _ => 1,
    }
}

fn print_json<T: Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("response serialization must succeed")
    );
}

fn main() {
    let config = match parse_args() {
        Ok(config) => config,
        Err(message) => {
            print_json(&ErrorResponse {
                repo: None,
                error: message,
            });
            std::process::exit(2);
        }
    };

    let mode = mode_for_config(&config);
    let repo_display = config.repo.display().to_string();
    let result = if config.dry_run {
        preview_next_ready_work(&config.repo, config.limit)
    } else {
        claim_next_ready_work(&config.repo, &config.claimant, &config.lease, config.limit)
    };

    match result {
        Ok(work) => {
            print_json(&BootstrapResponse {
                repo: repo_display,
                mode,
                selection_rule: SELECTION_RULE,
                work,
            });
        }
        Err(err) => {
            let exit_code = error_exit_code(&err);
            print_json(&ErrorResponse {
                repo: Some(repo_display),
                error: err.to_string(),
            });
            std::process::exit(exit_code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        error_exit_code, mode_for_config, parse_args_from, usage, DEFAULT_CLAIMANT, DEFAULT_LEASE,
    };
    use sandbox::self_directed_dispatch::{DispatchError, DEFAULT_READY_LIMIT};
    use std::path::PathBuf;

    #[test]
    fn parses_dry_run_args_with_repo_local_defaults() {
        let config = parse_args_from(vec![
            "--repo".to_string(),
            "/tmp/choiros-rs".to_string(),
            "--dry-run".to_string(),
        ])
        .expect("args should parse");

        assert_eq!(config.repo, PathBuf::from("/tmp/choiros-rs"));
        assert_eq!(config.claimant, DEFAULT_CLAIMANT);
        assert_eq!(config.lease, DEFAULT_LEASE);
        assert_eq!(config.limit, DEFAULT_READY_LIMIT);
        assert!(config.dry_run);
        assert_eq!(mode_for_config(&config), "dry_run");
    }

    #[test]
    fn parses_claim_mode_overrides() {
        let config = parse_args_from(vec![
            "--repo".to_string(),
            "/tmp/choiros-rs".to_string(),
            "--claimant".to_string(),
            "worker-02".to_string(),
            "--lease".to_string(),
            "30m".to_string(),
            "--limit".to_string(),
            "7".to_string(),
        ])
        .expect("args should parse");

        assert_eq!(config.claimant, "worker-02");
        assert_eq!(config.lease, "30m");
        assert_eq!(config.limit, 7);
        assert!(!config.dry_run);
        assert_eq!(mode_for_config(&config), "claim");
    }

    #[test]
    fn rejects_invalid_limit_values() {
        let error = parse_args_from(vec![
            "--repo".to_string(),
            "/tmp/choiros-rs".to_string(),
            "--limit".to_string(),
            "abc".to_string(),
        ])
        .expect_err("invalid limit should fail");

        assert_eq!(error, "invalid --limit value: abc");
    }

    #[test]
    fn missing_repo_reports_usage() {
        let error = parse_args_from(Vec::<String>::new()).expect_err("missing repo should fail");
        assert_eq!(error, format!("missing required --repo\n{}", usage()));
    }

    #[test]
    fn usage_mentions_dry_run_validation_mode() {
        assert!(usage().contains("--dry-run"));
    }

    #[test]
    fn exit_codes_match_retryable_dispatch_contract() {
        assert_eq!(error_exit_code(&DispatchError::NoReadyWork), 3);
        assert_eq!(error_exit_code(&DispatchError::NoClaimableWork), 3);
        assert_eq!(
            error_exit_code(&DispatchError::CommandFailed {
                command: "cogent work ready",
                message: "boom".to_string(),
            }),
            1
        );
    }
}
