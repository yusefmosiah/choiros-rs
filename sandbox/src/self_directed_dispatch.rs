use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

pub const DEFAULT_READY_LIMIT: usize = 50;
pub const SELECTION_RULE: &str =
    "smallest ready priority, then smallest ADR/work identifier, then title, then work_id";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReadyWorkItem {
    pub work_id: String,
    pub title: String,
    pub objective: String,
    pub kind: String,
    pub execution_state: String,
    pub approval_state: String,
    pub lock_state: String,
    #[serde(default)]
    pub priority: Option<u32>,
    #[serde(default)]
    pub claimed_by: Option<String>,
    #[serde(default)]
    pub claimed_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug)]
pub enum DispatchError {
    Io(std::io::Error),
    Json(serde_json::Error),
    CommandFailed {
        command: &'static str,
        message: String,
    },
    NoReadyWork,
    NoClaimableWork,
}

impl fmt::Display for DispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DispatchError::Io(err) => write!(f, "I/O error: {err}"),
            DispatchError::Json(err) => write!(f, "JSON parse error: {err}"),
            DispatchError::CommandFailed { command, message } => {
                write!(f, "{command} failed: {message}")
            }
            DispatchError::NoReadyWork => write!(f, "no ready work items found"),
            DispatchError::NoClaimableWork => {
                write!(
                    f,
                    "ready work items existed, but all claims raced or became unavailable"
                )
            }
        }
    }
}

impl std::error::Error for DispatchError {}

impl From<std::io::Error> for DispatchError {
    fn from(err: std::io::Error) -> Self {
        DispatchError::Io(err)
    }
}

impl From<serde_json::Error> for DispatchError {
    fn from(err: serde_json::Error) -> Self {
        DispatchError::Json(err)
    }
}

pub fn preview_next_ready_work(repo: &Path, limit: usize) -> Result<ReadyWorkItem, DispatchError> {
    sorted_ready_work(load_ready_work(repo, limit)?)
        .into_iter()
        .next()
        .ok_or(DispatchError::NoReadyWork)
}

pub fn claim_next_ready_work(
    repo: &Path,
    claimant: &str,
    lease: &str,
    limit: usize,
) -> Result<ReadyWorkItem, DispatchError> {
    let candidates = sorted_ready_work(load_ready_work(repo, limit)?);
    if candidates.is_empty() {
        return Err(DispatchError::NoReadyWork);
    }

    let mut saw_conflict = false;
    for candidate in candidates {
        match claim_work(repo, &candidate.work_id, claimant, lease) {
            Ok(claimed) => return Ok(claimed),
            Err(DispatchError::CommandFailed { command, message })
                if command == "cagent work claim" && claim_conflicted(&message) =>
            {
                saw_conflict = true;
            }
            Err(err) => return Err(err),
        }
    }

    if saw_conflict {
        Err(DispatchError::NoClaimableWork)
    } else {
        Err(DispatchError::NoReadyWork)
    }
}

pub fn sorted_ready_work(mut items: Vec<ReadyWorkItem>) -> Vec<ReadyWorkItem> {
    items.sort_by(compare_ready_work);
    items
}

fn load_ready_work(repo: &Path, limit: usize) -> Result<Vec<ReadyWorkItem>, DispatchError> {
    let limit_arg = limit.max(1).to_string();
    run_cagent_json(
        repo,
        "cagent work ready",
        &["work", "ready", "--json", "--limit", &limit_arg],
    )
}

fn claim_work(
    repo: &Path,
    work_id: &str,
    claimant: &str,
    lease: &str,
) -> Result<ReadyWorkItem, DispatchError> {
    run_cagent_json(
        repo,
        "cagent work claim",
        &[
            "work",
            "claim",
            work_id,
            "--json",
            "--claimant",
            claimant,
            "--lease",
            lease,
        ],
    )
}

fn run_cagent_json<T: DeserializeOwned>(
    repo: &Path,
    command_name: &'static str,
    args: &[&str],
) -> Result<T, DispatchError> {
    let output = Command::new("cagent")
        .args(args)
        .current_dir(repo)
        .output()?;

    if !output.status.success() {
        return Err(DispatchError::CommandFailed {
            command: command_name,
            message: combined_output(&output.stdout, &output.stderr),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(serde_json::from_str(stdout.trim())?)
}

fn combined_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let combined = format!("{}\n{}", stdout.trim(), stderr.trim());
    combined
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn claim_conflicted(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("resource busy")
        || normalized.contains("claimed by another worker")
        || normalized.contains("already claimed")
        || normalized.contains("not ready")
}

fn compare_ready_work(left: &ReadyWorkItem, right: &ReadyWorkItem) -> Ordering {
    left.priority
        .unwrap_or(u32::MAX)
        .cmp(&right.priority.unwrap_or(u32::MAX))
        .then_with(|| compare_numeric_identifier(left, right))
        .then_with(|| {
            left.title
                .to_ascii_lowercase()
                .cmp(&right.title.to_ascii_lowercase())
        })
        .then_with(|| left.work_id.cmp(&right.work_id))
}

fn compare_numeric_identifier(left: &ReadyWorkItem, right: &ReadyWorkItem) -> Ordering {
    match (
        extract_numeric_identifier(left),
        extract_numeric_identifier(right),
    ) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn extract_numeric_identifier(item: &ReadyWorkItem) -> Option<u32> {
    extract_numeric_identifier_from_text(&item.title)
        .or_else(|| extract_numeric_identifier_from_text(&item.objective))
}

fn extract_numeric_identifier_from_text(text: &str) -> Option<u32> {
    static IDENTIFIER_RE: OnceLock<regex::Regex> = OnceLock::new();
    let regex = IDENTIFIER_RE.get_or_init(|| {
        regex::Regex::new(r"(?i)\b(?:adr|work)[-_ ]*0*([0-9]+)\b")
            .expect("identifier regex must compile")
    });

    regex
        .captures(text)
        .and_then(|captures| captures.get(1))
        .and_then(|matched| matched.as_str().parse::<u32>().ok())
}

#[cfg(test)]
mod tests {
    use super::{extract_numeric_identifier_from_text, sorted_ready_work, ReadyWorkItem};

    fn work_item(work_id: &str, title: &str, objective: &str, priority: u32) -> ReadyWorkItem {
        ReadyWorkItem {
            work_id: work_id.to_string(),
            title: title.to_string(),
            objective: objective.to_string(),
            kind: "plan".to_string(),
            execution_state: "ready".to_string(),
            approval_state: "none".to_string(),
            lock_state: "unlocked".to_string(),
            priority: Some(priority),
            claimed_by: None,
            claimed_until: None,
            created_at: "2026-03-16T00:00:00Z".to_string(),
            updated_at: "2026-03-16T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn extracts_adr_identifier_from_title_or_objective() {
        assert_eq!(
            extract_numeric_identifier_from_text("ADR-0026: Self-Directing Agent Dispatch"),
            Some(26)
        );
        assert_eq!(
            extract_numeric_identifier_from_text(
                "docs/theory/decisions/adr-0024-hypervisor-go-rewrite.md"
            ),
            Some(24)
        );
        assert_eq!(
            extract_numeric_identifier_from_text("Declarative worker VM setup"),
            None
        );
    }

    #[test]
    fn sorts_by_priority_before_identifier() {
        let items = vec![
            work_item(
                "work_b",
                "ADR-0024: ChoirOS Go Rewrite",
                "docs/theory/decisions/adr-0024-hypervisor-go-rewrite.md",
                2,
            ),
            work_item(
                "work_a",
                "ADR-0020: Security Hardening",
                "docs/theory/decisions/adr-0020-security-hardening.md",
                1,
            ),
        ];

        let sorted = sorted_ready_work(items);
        assert_eq!(sorted[0].title, "ADR-0020: Security Hardening");
        assert_eq!(sorted[1].title, "ADR-0024: ChoirOS Go Rewrite");
    }

    #[test]
    fn sorts_same_priority_by_identifier_before_title() {
        let items = vec![
            work_item(
                "work_b",
                "ADR-0024: ChoirOS Go Rewrite",
                "docs/theory/decisions/adr-0024-hypervisor-go-rewrite.md",
                2,
            ),
            work_item(
                "work_c",
                "ADR-0014: Per-User VM Lifecycle",
                "docs/theory/decisions/adr-0014-per-user-storage-and-desktop-sync.md",
                2,
            ),
            work_item(
                "work_a",
                "ADR-0026: Self-Directing Agent Dispatch",
                "docs/theory/decisions/adr-0026-self-directing-agent-dispatch.md",
                2,
            ),
        ];

        let sorted = sorted_ready_work(items);
        assert_eq!(sorted[0].title, "ADR-0014: Per-User VM Lifecycle");
        assert_eq!(sorted[1].title, "ADR-0024: ChoirOS Go Rewrite");
        assert_eq!(sorted[2].title, "ADR-0026: Self-Directing Agent Dispatch");
    }

    #[test]
    fn falls_back_to_title_and_work_id_when_identifier_is_missing() {
        let items = vec![
            work_item(
                "work_02",
                "Terminal UX: root user, wrong cwd, no prompt, cagent not installed",
                "Fix terminal UX drift",
                2,
            ),
            work_item(
                "work_01",
                "Declarative worker VM setup: cagent + adapters via NixOS",
                "Set up worker VMs",
                2,
            ),
        ];

        let sorted = sorted_ready_work(items);
        assert_eq!(
            sorted[0].title,
            "Declarative worker VM setup: cagent + adapters via NixOS"
        );
        assert_eq!(
            sorted[1].title,
            "Terminal UX: root user, wrong cwd, no prompt, cagent not installed"
        );
    }
}
