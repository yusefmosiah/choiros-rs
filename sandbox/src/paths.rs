//! Centralized path resolution for sandbox runtime.
//!
//! In production (NixOS VM), all paths come from environment variables set by
//! the systemd unit. In local dev (debug builds), falls back to CARGO_MANIFEST_DIR.

use std::path::PathBuf;

/// Resolve a required path from an environment variable.
/// In debug builds, falls back to CARGO_MANIFEST_DIR.
/// In release builds, panics if the env var is not set.
fn env_path(var: &str) -> PathBuf {
    match std::env::var(var) {
        Ok(val) => PathBuf::from(val),
        Err(_) => {
            if cfg!(debug_assertions) {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            } else {
                panic!("{var} must be set in production")
            }
        }
    }
}

/// Root directory for sandbox data (user files, databases, etc).
pub fn sandbox_root() -> PathBuf {
    env_path("CHOIR_SANDBOX_ROOT")
}

/// Root directory for writer documents (draft.md, revisions).
/// Falls back to sandbox_root if CHOIR_WRITER_ROOT_DIR is not set.
pub fn writer_root() -> PathBuf {
    std::env::var("CHOIR_WRITER_ROOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| sandbox_root())
}

/// Root directory for user workspace files (projects, user-created content).
/// Falls back to sandbox_root if CHOIR_WORKSPACE_DIR is not set.
/// Separate from sandbox_root to support runtime/workspace data separation.
pub fn workspace_dir() -> PathBuf {
    std::env::var("CHOIR_WORKSPACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| sandbox_root())
}
