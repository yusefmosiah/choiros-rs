//! Writer API endpoints
//!
//! Provides document editing with optimistic concurrency control via revision tracking.
//! All paths are constrained to the sandbox directory.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use pulldown_cmark::{Options, Parser};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use tokio::fs;

use crate::actors::event_store::{AppendEvent, EventStoreMsg};
use crate::actors::writer::{
    DocumentVersion, Overlay, OverlayStatus, VersionSource, WriterError, WriterMsg,
};
use crate::api::ApiState;

/// Sandbox root path - all file operations are constrained to this directory
fn sandbox_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Writer error codes for machine-readable error responses
#[derive(Debug, Clone)]
pub enum WriterErrorCode {
    PathTraversal,
    NotFound,
    IsDirectory,
    InvalidRevision,
    Conflict,
    ReadError,
    WriteError,
}

impl WriterErrorCode {
    fn as_str(&self) -> &'static str {
        match self {
            WriterErrorCode::PathTraversal => "PATH_TRAVERSAL",
            WriterErrorCode::NotFound => "NOT_FOUND",
            WriterErrorCode::IsDirectory => "IS_DIRECTORY",
            WriterErrorCode::InvalidRevision => "INVALID_REVISION",
            WriterErrorCode::Conflict => "CONFLICT",
            WriterErrorCode::ReadError => "READ_ERROR",
            WriterErrorCode::WriteError => "WRITE_ERROR",
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            WriterErrorCode::PathTraversal => StatusCode::FORBIDDEN,
            WriterErrorCode::NotFound => StatusCode::NOT_FOUND,
            WriterErrorCode::IsDirectory => StatusCode::BAD_REQUEST,
            WriterErrorCode::InvalidRevision => StatusCode::BAD_REQUEST,
            WriterErrorCode::Conflict => StatusCode::CONFLICT,
            WriterErrorCode::ReadError => StatusCode::INTERNAL_SERVER_ERROR,
            WriterErrorCode::WriteError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Error response structures
#[derive(Debug, Serialize)]
pub struct WriterErrorDetail {
    code: String,
    message: String,
}

#[derive(Debug, Serialize)]
pub struct WriterErrorResponse {
    error: WriterErrorDetail,
}

/// Create an error response
fn writer_error(code: WriterErrorCode, message: impl Into<String>) -> impl IntoResponse {
    let status = code.status_code();
    let body = Json(WriterErrorResponse {
        error: WriterErrorDetail {
            code: code.as_str().to_string(),
            message: message.into(),
        },
    });
    (status, body)
}

/// Validates and normalizes a path relative to sandbox
fn validate_path(sandbox: &Path, user_path: &str) -> Result<PathBuf, axum::response::Response> {
    // Reject null bytes
    if user_path.contains('\0') {
        return Err(
            writer_error(WriterErrorCode::PathTraversal, "Path contains null bytes")
                .into_response(),
        );
    }

    // Reject absolute paths
    if user_path.starts_with('/') {
        return Err(writer_error(
            WriterErrorCode::PathTraversal,
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
                    return Err(writer_error(
                        WriterErrorCode::PathTraversal,
                        "Path escapes sandbox directory",
                    )
                    .into_response());
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(writer_error(
                    WriterErrorCode::PathTraversal,
                    "Path contains invalid components",
                )
                .into_response());
            }
        }
    }

    // Join with sandbox
    let full_path = sandbox.join(&normalized);

    // Canonicalize if the path exists
    let canonical = if full_path.exists() {
        match full_path.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                return Err(writer_error(
                    WriterErrorCode::ReadError,
                    format!("Failed to canonicalize path: {e}"),
                )
                .into_response());
            }
        }
    } else {
        full_path.clone()
    };

    // Ensure the path is still within the sandbox
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
            return Err(writer_error(
                WriterErrorCode::ReadError,
                format!("Failed to canonicalize sandbox root: {e}"),
            )
            .into_response());
        }
    };

    if !path_to_check.starts_with(&sandbox_canonical) {
        return Err(writer_error(
            WriterErrorCode::PathTraversal,
            "Path escapes sandbox directory",
        )
        .into_response());
    }

    Ok(full_path)
}

/// Get MIME type based on file extension
fn get_mime_type(path: &str) -> String {
    if path.ends_with(".md") || path.ends_with(".markdown") {
        "text/markdown".to_string()
    } else if path.ends_with(".txt") {
        "text/plain".to_string()
    } else if path.ends_with(".rs") {
        "text/rust".to_string()
    } else if path.ends_with(".json") {
        "application/json".to_string()
    } else if path.ends_with(".html") || path.ends_with(".htm") {
        "text/html".to_string()
    } else if path.ends_with(".css") {
        "text/css".to_string()
    } else if path.ends_with(".js") {
        "text/javascript".to_string()
    } else if path.ends_with(".toml") {
        "text/toml".to_string()
    } else if path.ends_with(".yaml") || path.ends_with(".yml") {
        "text/yaml".to_string()
    } else {
        "text/plain".to_string()
    }
}

/// Request to open a document
#[derive(Debug, Deserialize)]
pub struct OpenDocumentRequest {
    pub path: String,
}

/// Response for successful document open
#[derive(Debug, Serialize)]
pub struct OpenDocumentResponse {
    pub path: String,
    pub content: String,
    pub mime: String,
    pub revision: u64,
    pub readonly: bool,
}

/// Open a document for editing
pub async fn open_document(
    State(state): State<ApiState>,
    Json(req): Json<OpenDocumentRequest>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = req.path;

    let file_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if path exists and is a file
    match fs::metadata(&file_path).await {
        Ok(m) => {
            if m.is_dir() {
                return writer_error(
                    WriterErrorCode::IsDirectory,
                    format!("Path is a directory, not a file: {user_path}"),
                )
                .into_response();
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return writer_error(
                WriterErrorCode::NotFound,
                format!("File not found: {user_path}"),
            )
            .into_response();
        }
        Err(e) => {
            return writer_error(
                WriterErrorCode::ReadError,
                format!("Failed to read file metadata: {e}"),
            )
            .into_response();
        }
    };

    // Read file content
    let bytes = match fs::read(&file_path).await {
        Ok(b) => b,
        Err(e) => {
            return writer_error(
                WriterErrorCode::ReadError,
                format!("Failed to read file: {e}"),
            )
            .into_response();
        }
    };

    // Check for binary content
    if bytes.contains(&0) {
        return writer_error(WriterErrorCode::ReadError, "Binary files are not supported")
            .into_response();
    }

    let content = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            return writer_error(WriterErrorCode::ReadError, "File contains invalid UTF-8")
                .into_response();
        }
    };

    // Get or create revision
    let revision = match get_or_create_revision(&state, &user_path).await {
        Ok(rev) => rev,
        Err(e) => {
            return writer_error(WriterErrorCode::ReadError, e).into_response();
        }
    };

    let mime = get_mime_type(&user_path);

    // Check if file is readonly
    let readonly = match fs::metadata(&file_path).await {
        Ok(m) => m.permissions().readonly(),
        Err(_) => false,
    };

    (
        StatusCode::OK,
        Json(OpenDocumentResponse {
            path: user_path,
            content,
            mime,
            revision,
            readonly,
        }),
    )
        .into_response()
}

/// Request to save a document
#[derive(Debug, Deserialize)]
pub struct SaveDocumentRequest {
    pub path: String,
    pub base_rev: u64,
    pub content: String,
}

/// Response for successful document save
#[derive(Debug, Serialize)]
pub struct SaveDocumentResponse {
    pub path: String,
    pub revision: u64,
    pub saved: bool,
}

/// Conflict response includes current server state
#[derive(Debug, Serialize)]
pub struct ConflictResponse {
    #[serde(flatten)]
    error: WriterErrorResponse,
    pub path: String,
    pub current_revision: u64,
    pub current_content: String,
}

/// Save document with optimistic concurrency control
pub async fn save_document(
    State(state): State<ApiState>,
    Json(req): Json<SaveDocumentRequest>,
) -> impl IntoResponse {
    let sandbox = sandbox_root();
    let user_path = req.path;

    let file_path = match validate_path(&sandbox, &user_path) {
        Ok(p) => p,
        Err(response) => return response,
    };

    // Check if path exists and is a file (or doesn't exist yet)
    let file_exists = file_path.exists();
    if file_exists {
        match fs::metadata(&file_path).await {
            Ok(m) => {
                if m.is_dir() {
                    return writer_error(
                        WriterErrorCode::IsDirectory,
                        format!("Path is a directory: {user_path}"),
                    )
                    .into_response();
                }
            }
            Err(e) => {
                return writer_error(
                    WriterErrorCode::ReadError,
                    format!("Failed to read file metadata: {e}"),
                )
                .into_response();
            }
        };
    }

    // "Save As" on a new file may use base_rev=0 from the client.
    // Accept this explicitly and initialize revision to 1 on first write.
    if !file_exists && (req.base_rev == 0 || req.base_rev == 1) {
        if let Err(e) = fs::write(&file_path, &req.content).await {
            return writer_error(
                WriterErrorCode::WriteError,
                format!("Failed to write file: {e}"),
            )
            .into_response();
        }

        if let Err(e) = set_revision(&state, &user_path, 1).await {
            return writer_error(WriterErrorCode::WriteError, e).into_response();
        }

        return (
            StatusCode::OK,
            Json(SaveDocumentResponse {
                path: user_path,
                revision: 1,
                saved: true,
            }),
        )
            .into_response();
    }

    // Get current revision
    let current_revision = match get_or_create_revision(&state, &user_path).await {
        Ok(rev) => rev,
        Err(e) => {
            return writer_error(WriterErrorCode::ReadError, e).into_response();
        }
    };

    // Check for conflict
    if req.base_rev != current_revision {
        // Read current content for conflict response
        let current_content = if file_exists {
            match fs::read_to_string(&file_path).await {
                Ok(c) => c,
                Err(e) => {
                    return writer_error(
                        WriterErrorCode::ReadError,
                        format!("Failed to read current file content: {e}"),
                    )
                    .into_response();
                }
            }
        } else {
            String::new()
        };

        let response = Json(ConflictResponse {
            error: WriterErrorResponse {
                error: WriterErrorDetail {
                    code: WriterErrorCode::Conflict.as_str().to_string(),
                    message: "Document was modified by another client".to_string(),
                },
            },
            path: user_path,
            current_revision,
            current_content,
        });
        return (StatusCode::CONFLICT, response).into_response();
    }

    // Write the file
    if let Err(e) = fs::write(&file_path, &req.content).await {
        return writer_error(
            WriterErrorCode::WriteError,
            format!("Failed to write file: {e}"),
        )
        .into_response();
    }

    // Increment revision
    let new_revision = match increment_revision(&state, &user_path).await {
        Ok(rev) => rev,
        Err(e) => {
            return writer_error(WriterErrorCode::WriteError, e).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(SaveDocumentResponse {
            path: user_path,
            revision: new_revision,
            saved: true,
        }),
    )
        .into_response()
}

/// Request to preview markdown
#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub path: Option<String>,
    pub content: Option<String>,
}

/// Response for markdown preview
#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    pub html: String,
}

/// Request to submit a writer prompt for a run document.
#[derive(Debug, Deserialize)]
pub struct PromptDocumentRequest {
    pub path: String,
    pub prompt_diff: Vec<shared_types::PatchOp>,
    pub base_version_id: u64,
}

/// Response for successful writer prompt enqueue.
#[derive(Debug, Serialize)]
pub struct PromptDocumentResponse {
    pub run_id: String,
    pub message_id: String,
    pub revision: u64,
    pub queue_len: usize,
    pub duplicate: bool,
}

#[derive(Debug, Deserialize)]
pub struct VersionListQuery {
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct VersionQuery {
    pub path: String,
    pub version_id: u64,
}

#[derive(Debug, Serialize)]
pub struct ListVersionsResponse {
    pub run_id: String,
    pub head_version_id: u64,
    pub versions: Vec<DocumentVersion>,
}

#[derive(Debug, Serialize)]
pub struct GetVersionResponse {
    pub run_id: String,
    pub version: DocumentVersion,
    pub overlays: Vec<Overlay>,
}

#[derive(Debug, Deserialize)]
pub struct SaveVersionRequest {
    pub path: String,
    pub content: String,
    pub parent_version_id: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct SaveVersionResponse {
    pub run_id: String,
    pub version: DocumentVersion,
    pub saved: bool,
}

/// Preview markdown content
pub async fn preview_markdown(
    State(_state): State<ApiState>,
    Json(req): Json<PreviewRequest>,
) -> impl IntoResponse {
    let content = match (&req.content, &req.path) {
        (Some(content), _) => content.clone(),
        (None, Some(path)) => {
            // Read content from file
            let sandbox = sandbox_root();
            let file_path = match validate_path(&sandbox, path) {
                Ok(p) => p,
                Err(response) => return response,
            };

            // Check if path exists and is a file
            match fs::metadata(&file_path).await {
                Ok(m) => {
                    if m.is_dir() {
                        return writer_error(
                            WriterErrorCode::IsDirectory,
                            format!("Path is a directory: {path}"),
                        )
                        .into_response();
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return writer_error(
                        WriterErrorCode::NotFound,
                        format!("File not found: {path}"),
                    )
                    .into_response();
                }
                Err(e) => {
                    return writer_error(
                        WriterErrorCode::ReadError,
                        format!("Failed to read file metadata: {e}"),
                    )
                    .into_response();
                }
            };

            // Read file content
            match fs::read(&file_path).await {
                Ok(b) => {
                    if b.contains(&0) {
                        return writer_error(
                            WriterErrorCode::ReadError,
                            "Binary files cannot be previewed",
                        )
                        .into_response();
                    }
                    match String::from_utf8(b) {
                        Ok(s) => s,
                        Err(_) => {
                            return writer_error(
                                WriterErrorCode::ReadError,
                                "File contains invalid UTF-8",
                            )
                            .into_response();
                        }
                    }
                }
                Err(e) => {
                    return writer_error(
                        WriterErrorCode::ReadError,
                        format!("Failed to read file: {e}"),
                    )
                    .into_response();
                }
            }
        }
        (None, None) => {
            return writer_error(
                WriterErrorCode::InvalidRevision,
                "Either path or content must be provided",
            )
            .into_response();
        }
    };

    let html = markdown_to_html(&content);

    (StatusCode::OK, Json(PreviewResponse { html })).into_response()
}

fn extract_run_id_from_document_path(path: &str) -> Option<String> {
    let mut parts = path.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("conductor"), Some("runs"), Some(run_id), Some("draft.md"), None) => {
            Some(run_id.to_string())
        }
        _ => None,
    }
}

fn map_writer_actor_error(error: WriterError) -> axum::response::Response {
    match error {
        WriterError::Validation(message) => {
            if message.contains("not found") {
                writer_error(WriterErrorCode::NotFound, message).into_response()
            } else {
                writer_error(WriterErrorCode::InvalidRevision, message).into_response()
            }
        }
        WriterError::ActorUnavailable(message) => {
            writer_error(WriterErrorCode::WriteError, message).into_response()
        }
        other => writer_error(WriterErrorCode::WriteError, other.to_string()).into_response(),
    }
}

async fn ensure_conductor_writer_actor(
    state: &ApiState,
    run_id: &str,
) -> Result<ractor::ActorRef<WriterMsg>, axum::response::Response> {
    state
        .app_state
        .ensure_run_writer(run_id)
        .await
        .map_err(|err| {
            writer_error(
                WriterErrorCode::WriteError,
                format!("Writer unavailable: {err}"),
            )
            .into_response()
        })
}

/// Submit a human prompt into the writer actor queue for the run document.
pub async fn prompt_document(
    State(state): State<ApiState>,
    Json(req): Json<PromptDocumentRequest>,
) -> impl IntoResponse {
    if req.prompt_diff.is_empty() {
        return writer_error(
            WriterErrorCode::InvalidRevision,
            "prompt_diff cannot be empty",
        )
        .into_response();
    }

    let run_id = match extract_run_id_from_document_path(req.path.trim()) {
        Some(run_id) => run_id,
        None => {
            return writer_error(
                WriterErrorCode::InvalidRevision,
                "Prompting requires a run document path: conductor/runs/{run_id}/draft.md",
            )
            .into_response();
        }
    };

    let _ = state
        .app_state
        .event_store()
        .cast(EventStoreMsg::AppendAsync {
            event: AppendEvent {
                event_type: "user_input".to_string(),
                payload: serde_json::json!({
                    "surface": "writer.prompt_document",
                    "run_id": &run_id,
                    "path": &req.path,
                    "base_version_id": req.base_version_id,
                    "prompt_diff_len": req.prompt_diff.len(),
                }),
                actor_id: "api.writer".to_string(),
                user_id: "system".to_string(),
            },
        });

    let writer_actor = match ensure_conductor_writer_actor(&state, &run_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let ack = match ractor::call!(writer_actor, |reply| WriterMsg::SubmitUserPrompt {
        run_id: run_id.clone(),
        prompt_diff: req.prompt_diff.clone(),
        base_version_id: req.base_version_id,
        reply,
    }) {
        Ok(Ok(ack)) => ack,
        Ok(Err(err)) => return map_writer_actor_error(err),
        Err(err) => {
            return writer_error(WriterErrorCode::WriteError, err.to_string()).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(PromptDocumentResponse {
            run_id,
            message_id: ack.message_id,
            revision: ack.revision,
            queue_len: ack.queue_len,
            duplicate: ack.duplicate,
        }),
    )
        .into_response()
}

/// List versions for a run document path.
pub async fn list_versions(
    State(state): State<ApiState>,
    Query(query): Query<VersionListQuery>,
) -> impl IntoResponse {
    let run_id = match extract_run_id_from_document_path(query.path.trim()) {
        Some(run_id) => run_id,
        None => {
            return writer_error(
                WriterErrorCode::InvalidRevision,
                "Version listing requires run document path: conductor/runs/{run_id}/draft.md",
            )
            .into_response();
        }
    };

    let writer_actor = match ensure_conductor_writer_actor(&state, &run_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let mut versions = match ractor::call!(writer_actor, |reply| {
        WriterMsg::ListWriterDocumentVersions {
            run_id: run_id.clone(),
            reply,
        }
    }) {
        Ok(Ok(versions)) => versions,
        Ok(Err(err)) => return map_writer_actor_error(err),
        Err(err) => {
            return writer_error(WriterErrorCode::WriteError, err.to_string()).into_response();
        }
    };

    versions.sort_by_key(|version| version.version_id);
    let head_version_id = versions
        .last()
        .map(|version| version.version_id)
        .unwrap_or(0);

    (
        StatusCode::OK,
        Json(ListVersionsResponse {
            run_id,
            head_version_id,
            versions,
        }),
    )
        .into_response()
}

/// Get a specific version and overlays for a run document path.
pub async fn get_version(
    State(state): State<ApiState>,
    Query(query): Query<VersionQuery>,
) -> impl IntoResponse {
    let run_id = match extract_run_id_from_document_path(query.path.trim()) {
        Some(run_id) => run_id,
        None => {
            return writer_error(
                WriterErrorCode::InvalidRevision,
                "Version fetch requires run document path: conductor/runs/{run_id}/draft.md",
            )
            .into_response();
        }
    };

    let writer_actor = match ensure_conductor_writer_actor(&state, &run_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let version = match ractor::call!(writer_actor.clone(), |reply| {
        WriterMsg::GetWriterDocumentVersion {
            run_id: run_id.clone(),
            version_id: query.version_id,
            reply,
        }
    }) {
        Ok(Ok(version)) => version,
        Ok(Err(err)) => return map_writer_actor_error(err),
        Err(err) => {
            return writer_error(WriterErrorCode::WriteError, err.to_string()).into_response();
        }
    };

    let overlays = match ractor::call!(writer_actor, |reply| {
        WriterMsg::ListWriterDocumentOverlays {
            run_id: run_id.clone(),
            base_version_id: Some(query.version_id),
            status: Some(OverlayStatus::Pending),
            reply,
        }
    }) {
        Ok(Ok(overlays)) => overlays,
        Ok(Err(err)) => return map_writer_actor_error(err),
        Err(err) => {
            return writer_error(WriterErrorCode::WriteError, err.to_string()).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(GetVersionResponse {
            run_id,
            version,
            overlays,
        }),
    )
        .into_response()
}

/// Save current editor content as a new user-sourced run version.
pub async fn save_version(
    State(state): State<ApiState>,
    Json(req): Json<SaveVersionRequest>,
) -> impl IntoResponse {
    if req.content.trim().is_empty() {
        return writer_error(WriterErrorCode::InvalidRevision, "content cannot be empty")
            .into_response();
    }

    let run_id = match extract_run_id_from_document_path(req.path.trim()) {
        Some(run_id) => run_id,
        None => {
            return writer_error(
                WriterErrorCode::InvalidRevision,
                "Save version requires run document path: conductor/runs/{run_id}/draft.md",
            )
            .into_response();
        }
    };

    let writer_actor = match ensure_conductor_writer_actor(&state, &run_id).await {
        Ok(actor) => actor,
        Err(response) => return response,
    };

    let parent_version_id = if let Some(parent) = req.parent_version_id {
        Some(parent)
    } else {
        let versions = match ractor::call!(writer_actor.clone(), |reply| {
            WriterMsg::ListWriterDocumentVersions {
                run_id: run_id.clone(),
                reply,
            }
        }) {
            Ok(Ok(versions)) => versions,
            Ok(Err(err)) => return map_writer_actor_error(err),
            Err(err) => {
                return writer_error(WriterErrorCode::WriteError, err.to_string()).into_response();
            }
        };
        versions.iter().map(|v| v.version_id).max()
    };

    let version = match ractor::call!(writer_actor, |reply| {
        WriterMsg::CreateWriterDocumentVersion {
            run_id: run_id.clone(),
            parent_version_id,
            content: req.content.clone(),
            source: VersionSource::UserSave,
            reply,
        }
    }) {
        Ok(Ok(version)) => version,
        Ok(Err(err)) => return map_writer_actor_error(err),
        Err(err) => {
            return writer_error(WriterErrorCode::WriteError, err.to_string()).into_response();
        }
    };

    (
        StatusCode::OK,
        Json(SaveVersionResponse {
            run_id,
            version,
            saved: true,
        }),
    )
        .into_response()
}

/// Convert markdown to HTML
fn markdown_to_html(content: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let parser = Parser::new_ext(content, options);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

/// Get or create revision for a document
async fn get_or_create_revision(_state: &ApiState, path: &str) -> Result<u64, String> {
    // Using file-based revision tracking via sidecar files
    // This avoids needing direct database access from the API layer
    let path = path.to_string();
    get_revision_from_sidecar(path).await
}

/// Increment revision for a document
async fn increment_revision(_state: &ApiState, path: &str) -> Result<u64, String> {
    let path = path.to_string();
    increment_revision_in_sidecar(path).await
}

/// Set a specific revision for a document in sidecar storage.
async fn set_revision(_state: &ApiState, path: &str, revision: u64) -> Result<(), String> {
    let path = path.to_string();
    set_revision_in_sidecar(path, revision).await
}

/// Sidecar file path for revision tracking
fn revision_sidecar_path(doc_path: &str) -> PathBuf {
    let sandbox = sandbox_root();
    // Store revisions in a hidden directory
    let rev_dir = sandbox.join(".writer_revisions");
    // Use the document path as the filename (with slashes replaced)
    let safe_name = doc_path.replace('/', "__");
    rev_dir.join(format!("{}.rev", safe_name))
}

/// Get revision from sidecar file
async fn get_revision_from_sidecar(path: String) -> Result<u64, String> {
    let sidecar = revision_sidecar_path(&path);

    // Ensure parent directory exists
    if let Some(parent) = sidecar.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return Err(format!("Failed to create revision directory: {e}"));
            }
        }
    }

    // Read existing revision or default to 1
    if sidecar.exists() {
        match fs::read_to_string(&sidecar).await {
            Ok(content) => content
                .trim()
                .parse::<u64>()
                .map_err(|e| format!("Invalid revision format: {e}")),
            Err(e) => Err(format!("Failed to read revision: {e}")),
        }
    } else {
        // First time opening this file, initialize with revision 1
        if let Err(e) = fs::write(&sidecar, "1").await {
            return Err(format!("Failed to initialize revision: {e}"));
        }
        Ok(1)
    }
}

/// Increment revision in sidecar file
async fn increment_revision_in_sidecar(path: String) -> Result<u64, String> {
    let sidecar = revision_sidecar_path(&path);

    // Get current revision
    let current = get_revision_from_sidecar(path).await?;
    let new_revision = current + 1;

    // Write new revision
    if let Err(e) = fs::write(&sidecar, new_revision.to_string()).await {
        return Err(format!("Failed to write revision: {e}"));
    }

    Ok(new_revision)
}

/// Set revision in sidecar file.
async fn set_revision_in_sidecar(path: String, revision: u64) -> Result<(), String> {
    let sidecar = revision_sidecar_path(&path);

    if let Some(parent) = sidecar.parent() {
        if !parent.exists() {
            if let Err(e) = fs::create_dir_all(parent).await {
                return Err(format!("Failed to create revision directory: {e}"));
            }
        }
    }

    if let Err(e) = fs::write(&sidecar, revision.to_string()).await {
        return Err(format!("Failed to write revision: {e}"));
    }
    Ok(())
}
