//! File tool execution for Conductor
//!
//! Provides sandboxed file operations for the Conductor to maintain its living document.
//! Similar to ResearcherAdapter file tools but tailored for Conductor's needs.

use std::path::{Path, PathBuf};

use crate::actors::conductor::protocol::ConductorError;

/// Sandbox root for file operations
fn sandbox_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Validate path is within sandbox
pub fn validate_sandbox_path(user_path: &str) -> Result<PathBuf, ConductorError> {
    // Reject absolute paths
    if user_path.starts_with('/') || user_path.starts_with('\\') || user_path.contains(':') {
        return Err(ConductorError::InvalidRequest(
            "Absolute paths not allowed".to_string(),
        ));
    }

    // Reject path traversal
    if user_path.contains("..") {
        return Err(ConductorError::InvalidRequest(
            "Path traversal not allowed".to_string(),
        ));
    }

    let sandbox = sandbox_root();
    let full_path = sandbox.join(user_path);

    // Ensure it's still within sandbox
    let canonical = full_path.canonicalize().unwrap_or(full_path.clone());
    let sandbox_canonical = sandbox.canonicalize().unwrap_or(sandbox.clone());

    if !canonical.starts_with(&sandbox_canonical) {
        return Err(ConductorError::InvalidRequest(
            "Path escapes sandbox".to_string(),
        ));
    }

    Ok(full_path)
}

/// Read a file within the sandbox
pub async fn file_read(path: &str) -> Result<String, ConductorError> {
    let full_path = validate_sandbox_path(path)?;

    tokio::fs::read_to_string(&full_path)
        .await
        .map_err(|e| ConductorError::FileError(format!("Failed to read file: {e}")))
}

/// Write a file within the sandbox
pub async fn file_write(path: &str, content: &str) -> Result<(), ConductorError> {
    let full_path = validate_sandbox_path(path)?;

    // Ensure parent directory exists
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ConductorError::FileError(format!("Failed to create directory: {e}")))?;
    }

    tokio::fs::write(&full_path, content)
        .await
        .map_err(|e| ConductorError::FileError(format!("Failed to write file: {e}")))
}

/// Edit a file within the sandbox (find and replace)
pub async fn file_edit(path: &str, old_text: &str, new_text: &str) -> Result<(), ConductorError> {
    let full_path = validate_sandbox_path(path)?;

    let content = tokio::fs::read_to_string(&full_path)
        .await
        .map_err(|e| ConductorError::FileError(format!("Failed to read file: {e}")))?;

    let new_content = content.replace(old_text, new_text);

    if new_content == content {
        return Err(ConductorError::FileError(
            "old_text not found in file".to_string(),
        ));
    }

    tokio::fs::write(&full_path, &new_content)
        .await
        .map_err(|e| ConductorError::FileError(format!("Failed to write file: {e}")))?;

    Ok(())
}

/// Ensure the conductor runs directory exists
pub async fn ensure_runs_directory() -> Result<PathBuf, ConductorError> {
    let sandbox = sandbox_root();
    let runs_dir = sandbox.join("conductor").join("runs");

    tokio::fs::create_dir_all(&runs_dir)
        .await
        .map_err(|e| ConductorError::FileError(format!("Failed to create runs directory: {e}")))?;

    Ok(runs_dir)
}

/// Get the document path for a run
pub fn get_run_document_path(run_id: &str) -> String {
    format!("conductor/runs/{}/draft.md", run_id)
}

/// Create initial draft document for a run
pub async fn create_initial_draft(run_id: &str, objective: &str) -> Result<String, ConductorError> {
    let document_path = get_run_document_path(run_id);

    let initial_content = format!(
        r#"# {objective}

## Current Understanding

Run started with objective: {objective}

Run ID: `{run_id}`

## In Progress

- [ ] Bootstrap agenda

## Next Steps

Initializing...
"#,
        objective = objective,
        run_id = run_id
    );

    file_write(&document_path, &initial_content).await?;

    Ok(document_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[test]
    fn test_validate_sandbox_path_rejects_absolute() {
        assert!(validate_sandbox_path("/etc/passwd").is_err());
        assert!(validate_sandbox_path("C:\\Windows\\system32").is_err());
    }

    #[test]
    fn test_validate_sandbox_path_rejects_traversal() {
        assert!(validate_sandbox_path("../Cargo.toml").is_err());
        assert!(validate_sandbox_path("foo/../../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_sandbox_path_accepts_valid() {
        assert!(validate_sandbox_path("conductor/runs/123/draft.md").is_ok());
        assert!(validate_sandbox_path("reports/output.md").is_ok());
    }

    #[tokio::test]
    async fn test_file_write_and_read() {
        let test_path = "test_output/test_file.txt";
        let test_content = "Hello, World!";

        // Write
        file_write(test_path, test_content).await.unwrap();

        // Read back
        let read_content = file_read(test_path).await.unwrap();
        assert_eq!(read_content, test_content);

        // Cleanup
        let full_path = validate_sandbox_path(test_path).unwrap();
        let _ = tokio::fs::remove_file(&full_path).await;
        let _ = tokio::fs::remove_dir(full_path.parent().unwrap()).await;
    }

    #[tokio::test]
    async fn test_file_edit() {
        let test_path = "test_output/test_edit.txt";
        let initial_content = "Hello, World!";
        let old_text = "World";
        let new_text = "Universe";

        // Setup
        file_write(test_path, initial_content).await.unwrap();

        // Edit
        file_edit(test_path, old_text, new_text).await.unwrap();

        // Verify
        let read_content = file_read(test_path).await.unwrap();
        assert_eq!(read_content, "Hello, Universe!");

        // Cleanup
        let full_path = validate_sandbox_path(test_path).unwrap();
        let _ = tokio::fs::remove_file(&full_path).await;
        let _ = tokio::fs::remove_dir(full_path.parent().unwrap()).await;
    }
}
