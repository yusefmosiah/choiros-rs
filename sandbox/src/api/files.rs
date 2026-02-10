//! Files API endpoints
//!
//! Provides secure file system operations within the ChoirOS sandbox directory.
//! All paths are relative to the sandbox root and path traversal attempts are rejected.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use tokio::fs;

use crate::api::ApiState;

/// Sandbox root path - all file operations are constrained to this directory
fn sandbox_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Maximum file read size (1MB)
const MAX_READ_SIZE: usize = 1_048_576;

/// File error codes for machine-readable error responses
#[derive(Debug, Clone)]
pub enum FileErrorCode {
    PathTraversal,
    NotFound,
    NotAFile,
    NotADirectory,
    AlreadyExists,
    PermissionDenied,
    InvalidContent,
    InternalError,
}

impl FileErrorCode {
    fn as_str(&self) -> &'static str {
        match self {
            FileErrorCode::PathTraversal => "PATH_TRAVERSAL",
            FileErrorCode::NotFound => "NOT_FOUND",
            FileErrorCode::NotAFile => "NOT_A_FILE",
            FileErrorCode::NotADirectory => "NOT_A_DIRECTORY",
            FileErrorCode::AlreadyExists => "ALREADY_EXISTS",
            FileErrorCode::PermissionDenied => "PERMISSION_DENIED",
            FileErrorCode::InvalidContent => "INVALID_CONTENT",
            FileErrorCode::InternalError => "INTERNAL_ERROR",
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            FileErrorCode::PathTraversal => StatusCode::FORBIDDEN,
            FileErrorCode::NotFound => StatusCode::NOT_FOUND,
            FileErrorCode::NotAFile => StatusCode::BAD_REQUEST,
            FileErrorCode::NotADirectory => StatusCode::BAD_REQUEST,
            FileErrorCode::AlreadyExists => StatusCode::CONFLICT,
            FileErrorCode::PermissionDenied => StatusCode::FORBIDDEN,
            FileErrorCode::InvalidContent => StatusCode::BAD_REQUEST,
            FileErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Error response structure
#[derive(Debug, Serialize)]
pub struct FileErrorResponse {
    error: FileErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct FileErrorDetail {
    code: String,
    message: String,
}

/// Create an error response
fn file_error(code: FileErrorCode, message: impl Into<String>) -> impl IntoResponse {
    let status = code.status_code();
    let body = Json(FileErrorResponse {
        error: FileErrorDetail {
            code: code.as_str().to_string(),
            message: message.into(),
        },
    });
    (status, body)
}

/// Validates and normalizes a path relative to sandbox
///
/// Returns the full path within the sandbox if valid, or an error response if invalid.
fn validate_path(sandbox: &Path, user_path: &str) -> Result<PathBuf, axum::response::Response> {
    // Reject null bytes
    if user_path.contains('\0') {
        return Err(
            file_error(FileErrorCode::PathTraversal, "Path contains null bytes").into_response(),
        );
    }

    // Reject absolute paths
    if user_path.starts_with('/') {
        return Err(file_error(
            FileErrorCode::PathTraversal,
            "Absolute paths are not allowed",
        )
        .into_response());
    }

    // Normalize path by processing components manually
    let mut normalized = PathBuf::new();
    for comp in Path::new(user_path).components() {
        match comp {
            Component::Normal(s) => normalized.push(s),
            Component::CurDir => {} // Skip .
            Component::ParentDir => {
                // Pop the last component if we can, otherwise this escapes the sandbox
                if !normalized.pop() {
                    return Err(file_error(
                        FileErrorCode::PathTraversal,
                        "Path escapes sandbox directory",
                    )
                    .into_response());
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                // These shouldn't happen due to the absolute path check above,
                // but reject them just in case
                return Err(file_error(
                    FileErrorCode::PathTraversal,
                    "Path contains invalid components",
                )
                .into_response());
            }
        }
    }

    // Join with sandbox
    let full_path = sandbox.join(&normalized);

    // Canonicalize if the path exists, otherwise use the joined path
    // We need to check if the path exists first because canonicalize fails on non-existent paths
    let canonical = if full_path.exists() {
        match full_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                return Err(file_error(
                    FileErrorCode::InternalError,
                    format!("Failed to canonicalize path: {e}"),
                )
                .into_response());
            }
        }
    } else {
        // For non-existent paths, we still need to validate they don't escape the sandbox
        // by resolving any remaining .. components against the sandbox root
        full_path.clone()
    };

    // Ensure the path is still within the sandbox
    // For non-existent paths, we check if the parent directories stay within sandbox
    let path_to_check = if canonical.exists() {
        &canonical
    } else {
        // Check parent directories for non-existent paths
        let mut parent = canonical.as_path();
        while !parent.exists() {
            if let Some(p) = parent.parent() {
                parent = p;
            } else {
                break;
            }
        }
        parent
    };

    let sandbox_canonical = match sandbox.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return Err(file_error(
                FileErrorCode::InternalError,
                format!("Failed to canonicalize sandbox root: {e}"),
            )
            .into_response());
        }
    };

    if !path_to_check.starts_with(&sandbox_canonical) {
        return Err(file_error(
            FileErrorCode::PathTraversal,
            "Path escapes sandbox directory",
        )
        .into_response());
    }

    Ok(full_path)
}

/// Directory entry response
#[derive(Debug, Serialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    pub modified_at: String,
}

/// List directory response
#[derive(Debug, Serialize)]
pub struct ListDirectoryResponse {
    pub path: String,
    pub entries: Vec<DirectoryEntry>,
    pub total_count: usize,
}

/// List directory query parameters
#[derive(Debug, Deserialize)]
pub struct ListDirectoryQuery {
    pub path: Option<String>,
    pub recursive: Option<bool>,
}

/// List directory contents
pub async fn list_directory(
    State(_state): State<ApiState>,
    Query(query): Query<ListDirectoryQuery>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = query.path.unwrap_or_default();

    let dir_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if path exists and is a directory
    match fs::metadata(&dir_path).await {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return file_error(
                    FileErrorCode::NotADirectory,
                    format!("Path is not a directory: {user_path}"),
                )
                .into_response();
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return file_error(
                FileErrorCode::NotFound,
                format!("Directory not found: {user_path}"),
            )
            .into_response();
        }
        Err(e) => {
            return file_error(
                FileErrorCode::InternalError,
                format!("Failed to read directory metadata: {e}"),
            )
            .into_response();
        }
    }

    let recursive = query.recursive.unwrap_or(false);
    let mut entries = Vec::new();

    if recursive {
        match collect_entries_recursive(&dir_path, &sandbox, &user_path, &mut entries).await {
            Ok(_) => {}
            Err(e) => return e.into_response(),
        }
    } else {
        match collect_entries(&dir_path, &sandbox, &user_path).await {
            Ok(e) => entries = e,
            Err(e) => return e.into_response(),
        }
    }

    let total_count = entries.len();

    (
        StatusCode::OK,
        Json(ListDirectoryResponse {
            path: user_path,
            entries,
            total_count,
        }),
    )
        .into_response()
}

async fn collect_entries(
    dir_path: &Path,
    _sandbox: &Path,
    user_path: &str,
) -> Result<Vec<DirectoryEntry>, axum::response::Response> {
    let mut entries = Vec::new();
    let mut read_dir = match fs::read_dir(dir_path).await {
        Ok(rd) => rd,
        Err(e) => {
            return Err(file_error(
                FileErrorCode::InternalError,
                format!("Failed to read directory: {e}"),
            )
            .into_response());
        }
    };

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue, // Skip entries we can't read
        };

        let entry_user_path = if user_path.is_empty() {
            name.clone()
        } else {
            format!("{user_path}/{name}")
        };

        let modified_at = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        entries.push(DirectoryEntry {
            name,
            path: entry_user_path,
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified_at,
        });
    }

    Ok(entries)
}

async fn collect_entries_recursive(
    dir_path: &Path,
    sandbox: &Path,
    user_path: &str,
    entries: &mut Vec<DirectoryEntry>,
) -> Result<(), axum::response::Response> {
    // Use a stack-based approach to avoid async recursion issues
    let mut stack: Vec<(PathBuf, String)> = vec![(dir_path.to_path_buf(), user_path.to_string())];

    while let Some((current_path, current_user_path)) = stack.pop() {
        let dir_entries = match collect_entries(&current_path, sandbox, &current_user_path).await {
            Ok(e) => e,
            Err(_) => continue, // Skip directories we can't read
        };

        for entry in dir_entries {
            let is_dir = entry.is_dir;
            let entry_path = entry.path.clone();
            entries.push(entry);

            if is_dir {
                let child_path = sandbox.join(&entry_path);
                stack.push((child_path, entry_path));
            }
        }
    }

    Ok(())
}

/// File metadata response
#[derive(Debug, Serialize)]
pub struct FileMetadataResponse {
    pub name: String,
    pub path: String,
    pub is_file: bool,
    pub is_dir: bool,
    pub size: u64,
    pub created_at: String,
    pub modified_at: String,
    pub permissions: String,
}

/// Get file metadata query parameters
#[derive(Debug, Deserialize)]
pub struct MetadataQuery {
    pub path: String,
}

/// Get file/directory metadata
pub async fn get_metadata(
    State(_state): State<ApiState>,
    Query(query): Query<MetadataQuery>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = query.path;

    let file_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    let metadata = match fs::metadata(&file_path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return file_error(
                FileErrorCode::NotFound,
                format!("File or directory not found: {user_path}"),
            )
            .into_response();
        }
        Err(e) => {
            return file_error(
                FileErrorCode::InternalError,
                format!("Failed to read metadata: {e}"),
            )
            .into_response();
        }
    };

    let name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let created_at = metadata
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    #[cfg(unix)]
    let permissions = {
        use std::os::unix::fs::PermissionsExt;
        format!("{:o}", metadata.permissions().mode())
    };
    #[cfg(not(unix))]
    let permissions = "644".to_string();

    (
        StatusCode::OK,
        Json(FileMetadataResponse {
            name,
            path: user_path,
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            created_at,
            modified_at,
            permissions,
        }),
    )
        .into_response()
}

/// File content response
#[derive(Debug, Serialize)]
pub struct FileContentResponse {
    pub path: String,
    pub content: String,
    pub size: usize,
    pub is_truncated: bool,
    pub encoding: String,
}

/// Get file content query parameters
#[derive(Debug, Deserialize)]
pub struct ContentQuery {
    pub path: String,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

/// Read file content
pub async fn get_content(
    State(_state): State<ApiState>,
    Query(query): Query<ContentQuery>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = query.path;

    let file_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if path exists and is a file
    match fs::metadata(&file_path).await {
        Ok(m) => {
            if m.is_dir() {
                return file_error(
                    FileErrorCode::NotAFile,
                    format!("Path is a directory, not a file: {user_path}"),
                )
                .into_response();
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return file_error(
                FileErrorCode::NotFound,
                format!("File not found: {user_path}"),
            )
            .into_response();
        }
        Err(e) => {
            return file_error(
                FileErrorCode::InternalError,
                format!("Failed to read file metadata: {e}"),
            )
            .into_response();
        }
    };

    // Read file content
    let bytes = match fs::read(&file_path).await {
        Ok(b) => b,
        Err(e) => {
            return file_error(
                FileErrorCode::InternalError,
                format!("Failed to read file: {e}"),
            )
            .into_response();
        }
    };

    // Check for binary content (null bytes or invalid UTF-8)
    if bytes.contains(&0) {
        return file_error(
            FileErrorCode::InvalidContent,
            "Binary files are not supported",
        )
        .into_response();
    }

    let content = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            return file_error(FileErrorCode::InvalidContent, "File contains invalid UTF-8")
                .into_response();
        }
    };

    // Apply offset and limit
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(MAX_READ_SIZE).min(MAX_READ_SIZE);

    let content_bytes = content.as_bytes();
    let total_size = content_bytes.len();

    let sliced_content = if offset >= total_size {
        String::new()
    } else {
        let end = (offset + limit).min(total_size);
        String::from_utf8_lossy(&content_bytes[offset..end]).to_string()
    };

    let is_truncated = offset > 0 || total_size > offset + limit;
    let response_size = sliced_content.len();

    (
        StatusCode::OK,
        Json(FileContentResponse {
            path: user_path,
            content: sliced_content,
            size: response_size,
            is_truncated,
            encoding: "utf-8".to_string(),
        }),
    )
        .into_response()
}

/// Create file request
#[derive(Debug, Deserialize)]
pub struct CreateFileRequest {
    pub path: String,
    pub content: Option<String>,
    pub overwrite: Option<bool>,
}

/// Create file response
#[derive(Debug, Serialize)]
pub struct CreateFileResponse {
    pub path: String,
    pub created: bool,
    pub size: u64,
}

/// Create a new file
pub async fn create_file(
    State(_state): State<ApiState>,
    Json(req): Json<CreateFileRequest>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = req.path;

    let file_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if file already exists
    if file_path.exists() {
        let overwrite = req.overwrite.unwrap_or(false);
        if !overwrite {
            return file_error(
                FileErrorCode::AlreadyExists,
                format!("File already exists: {user_path}"),
            )
            .into_response();
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = file_path.parent() {
        if !parent.exists() {
            return file_error(
                FileErrorCode::NotADirectory,
                format!("Parent directory does not exist: {}", parent.display()),
            )
            .into_response();
        }
    }

    let content = req.content.unwrap_or_default();
    let size = content.len() as u64;

    // Write the file
    match fs::write(&file_path, content).await {
        Ok(_) => (
            StatusCode::OK,
            Json(CreateFileResponse {
                path: user_path,
                created: true,
                size,
            }),
        )
            .into_response(),
        Err(e) => file_error(
            FileErrorCode::InternalError,
            format!("Failed to create file: {e}"),
        )
        .into_response(),
    }
}

/// Write file request
#[derive(Debug, Deserialize)]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
    pub append: Option<bool>,
    pub create_if_missing: Option<bool>,
}

/// Write file response
#[derive(Debug, Serialize)]
pub struct WriteFileResponse {
    pub path: String,
    pub bytes_written: usize,
    pub size: u64,
}

/// Write or overwrite file content
pub async fn write_file(
    State(_state): State<ApiState>,
    Json(req): Json<WriteFileRequest>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = req.path;

    let file_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    let create_if_missing = req.create_if_missing.unwrap_or(true);
    let append = req.append.unwrap_or(false);

    // Check if file exists
    let file_exists = file_path.exists();
    if !file_exists && !create_if_missing {
        return file_error(
            FileErrorCode::NotFound,
            format!("File not found: {user_path}"),
        )
        .into_response();
    }

    // Check if path is a directory
    if file_exists {
        match fs::metadata(&file_path).await {
            Ok(m) if m.is_dir() => {
                return file_error(
                    FileErrorCode::NotAFile,
                    format!("Path is a directory: {user_path}"),
                )
                .into_response();
            }
            _ => {}
        }
    }

    let bytes_written = req.content.len();

    // Write or append to the file
    let result = if append && file_exists {
        match fs::OpenOptions::new().append(true).open(&file_path).await {
            Ok(mut file) => {
                use tokio::io::AsyncWriteExt;
                file.write_all(req.content.as_bytes()).await
            }
            Err(e) => Err(e),
        }
    } else {
        fs::write(&file_path, &req.content).await
    };

    match result {
        Ok(_) => {
            // Get the new file size
            let size = match fs::metadata(&file_path).await {
                Ok(m) => m.len(),
                Err(_) => bytes_written as u64,
            };

            (
                StatusCode::OK,
                Json(WriteFileResponse {
                    path: user_path,
                    bytes_written,
                    size,
                }),
            )
                .into_response()
        }
        Err(e) => file_error(
            FileErrorCode::InternalError,
            format!("Failed to write file: {e}"),
        )
        .into_response(),
    }
}

/// Create directory request
#[derive(Debug, Deserialize)]
pub struct CreateDirectoryRequest {
    pub path: String,
    pub recursive: Option<bool>,
}

/// Create directory response
#[derive(Debug, Serialize)]
pub struct CreateDirectoryResponse {
    pub path: String,
    pub created: bool,
}

/// Create a new directory
pub async fn create_directory(
    State(_state): State<ApiState>,
    Json(req): Json<CreateDirectoryRequest>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = req.path;

    let dir_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if directory already exists
    if dir_path.exists() {
        return file_error(
            FileErrorCode::AlreadyExists,
            format!("Directory already exists: {user_path}"),
        )
        .into_response();
    }

    let recursive = req.recursive.unwrap_or(true);

    // Create the directory
    let result = if recursive {
        fs::create_dir_all(&dir_path).await
    } else {
        fs::create_dir(&dir_path).await
    };

    match result {
        Ok(_) => (
            StatusCode::OK,
            Json(CreateDirectoryResponse {
                path: user_path,
                created: true,
            }),
        )
            .into_response(),
        Err(e) => file_error(
            FileErrorCode::InternalError,
            format!("Failed to create directory: {e}"),
        )
        .into_response(),
    }
}

/// Rename/move request
#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub source: String,
    pub target: String,
    pub overwrite: Option<bool>,
}

/// Rename/move response
#[derive(Debug, Serialize)]
pub struct RenameResponse {
    pub source: String,
    pub target: String,
    pub renamed: bool,
}

/// Rename or move a file or directory
pub async fn rename_file(
    State(_state): State<ApiState>,
    Json(req): Json<RenameRequest>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();

    let source_path = match validate_path(&sandbox, &req.source) {
        Ok(p) => p,
        Err(response) => return response,
    };

    let target_path = match validate_path(&sandbox, &req.target) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if source exists
    if !source_path.exists() {
        return file_error(
            FileErrorCode::NotFound,
            format!("Source not found: {}", req.source),
        )
        .into_response();
    }

    // Check if target exists
    let overwrite = req.overwrite.unwrap_or(false);
    if target_path.exists() && !overwrite {
        return file_error(
            FileErrorCode::AlreadyExists,
            format!("Target already exists: {}", req.target),
        )
        .into_response();
    }

    // Ensure target parent directory exists
    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            return file_error(
                FileErrorCode::NotADirectory,
                format!(
                    "Target parent directory does not exist: {}",
                    parent.display()
                ),
            )
            .into_response();
        }
    }

    // Perform the rename
    match fs::rename(&source_path, &target_path).await {
        Ok(_) => (
            StatusCode::OK,
            Json(RenameResponse {
                source: req.source,
                target: req.target,
                renamed: true,
            }),
        )
            .into_response(),
        Err(e) => file_error(
            FileErrorCode::InternalError,
            format!("Failed to rename: {e}"),
        )
        .into_response(),
    }
}

/// Delete request
#[derive(Debug, Deserialize)]
pub struct DeleteRequest {
    pub path: String,
    pub recursive: Option<bool>,
}

/// Delete response
#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub path: String,
    pub deleted: bool,
    #[serde(rename = "type")]
    pub entry_type: String,
}

/// Delete a file or directory
pub async fn delete_file(
    State(_state): State<ApiState>,
    Json(req): Json<DeleteRequest>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = req.path;

    let file_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if path exists
    let metadata = match fs::metadata(&file_path).await {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return file_error(
                FileErrorCode::NotFound,
                format!("Path not found: {user_path}"),
            )
            .into_response();
        }
        Err(e) => {
            return file_error(
                FileErrorCode::InternalError,
                format!("Failed to read path metadata: {e}"),
            )
            .into_response();
        }
    };

    let is_dir = metadata.is_dir();
    let entry_type = if is_dir { "directory" } else { "file" };

    // Delete the file or directory
    let result = if is_dir {
        let recursive = req.recursive.unwrap_or(false);
        if recursive {
            fs::remove_dir_all(&file_path).await
        } else {
            fs::remove_dir(&file_path).await
        }
    } else {
        fs::remove_file(&file_path).await
    };

    match result {
        Ok(_) => (
            StatusCode::OK,
            Json(DeleteResponse {
                path: user_path,
                deleted: true,
                entry_type: entry_type.to_string(),
            }),
        )
            .into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => file_error(
            FileErrorCode::NotAFile,
            "Directory is not empty (use recursive=true to delete)",
        )
        .into_response(),
        Err(e) => file_error(
            FileErrorCode::InternalError,
            format!("Failed to delete: {e}"),
        )
        .into_response(),
    }
}

/// Copy request
#[derive(Debug, Deserialize)]
pub struct CopyRequest {
    pub source: String,
    pub target: String,
    pub overwrite: Option<bool>,
}

/// Copy response
#[derive(Debug, Serialize)]
pub struct CopyResponse {
    pub source: String,
    pub target: String,
    pub copied: bool,
    pub size: u64,
}

/// Copy a file
pub async fn copy_file(
    State(_state): State<ApiState>,
    Json(req): Json<CopyRequest>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();

    let source_path = match validate_path(&sandbox, &req.source) {
        Ok(p) => p,
        Err(response) => return response,
    };

    let target_path = match validate_path(&sandbox, &req.target) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if source exists and is a file
    let metadata = match fs::metadata(&source_path).await {
        Ok(m) => {
            if m.is_dir() {
                return file_error(
                    FileErrorCode::NotAFile,
                    format!("Source is a directory, not a file: {}", req.source),
                )
                .into_response();
            }
            m
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return file_error(
                FileErrorCode::NotFound,
                format!("Source not found: {}", req.source),
            )
            .into_response();
        }
        Err(e) => {
            return file_error(
                FileErrorCode::InternalError,
                format!("Failed to read source metadata: {e}"),
            )
            .into_response();
        }
    };

    let size = metadata.len();

    // Check if target exists
    let overwrite = req.overwrite.unwrap_or(false);
    if target_path.exists() && !overwrite {
        return file_error(
            FileErrorCode::AlreadyExists,
            format!("Target already exists: {}", req.target),
        )
        .into_response();
    }

    // Copy the file
    match fs::copy(&source_path, &target_path).await {
        Ok(_) => (
            StatusCode::OK,
            Json(CopyResponse {
                source: req.source,
                target: req.target,
                copied: true,
                size,
            }),
        )
            .into_response(),
        Err(e) => {
            file_error(FileErrorCode::InternalError, format!("Failed to copy: {e}")).into_response()
        }
    }
}
