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

fn usage() -> &'static str {
    "Usage: repo-worker-bootstrap --repo <path> [--claimant <name>] [--lease <duration>] [--limit <n>] [--dry-run]"
}

fn parse_args() -> Result<Config, String> {
    let mut repo = None;
    let mut claimant = "repo-worker-bootstrap".to_string();
    let mut lease = "15m".to_string();
    let mut limit = DEFAULT_READY_LIMIT;
    let mut dry_run = false;

    let mut args = std::env::args().skip(1);
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

    let mode = if config.dry_run { "dry_run" } else { "claim" };
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
            let exit_code = match err {
                DispatchError::NoReadyWork | DispatchError::NoClaimableWork => 3,
                _ => 1,
            };
            print_json(&ErrorResponse {
                repo: Some(repo_display),
                error: err.to_string(),
            });
            std::process::exit(exit_code);
        }
    }
}
