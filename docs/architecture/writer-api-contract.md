# Writer App API Contract

## Overview

The Writer App API provides secure document editing capabilities with optimistic concurrency control via revision tracking. It is designed for collaborative editing scenarios where multiple clients may attempt to modify the same document simultaneously.

**Key Features:**
- **Revision-based concurrency**: Each save increments a monotonic revision counter
- **Optimistic locking**: Clients must provide `base_rev` when saving; conflicts return current server state
- **Markdown preview**: Server-side rendering with GitHub-flavored markdown
- **Sandbox security**: All paths are constrained to the ChoirOS sandbox directory

**Base URL**: `/writer`

---

## Philosophy

The Writer API follows these design principles:

1. **Explicit Concurrency**: Rather than last-write-wins, conflicts are surfaced to clients
2. **Stateless Operations**: Each request contains all context needed (path, revision, content)
3. **Fail Fast**: Path validation happens before any file operations
4. **Client Authority**: Clients are responsible for merge resolution during conflicts

---

## Path Normalization Rules

All paths are normalized according to the following rules:

1. **Relative to Sandbox**: All paths are relative to `/Users/wiz/choiros-rs/sandbox`
2. **Normalization**:
   - Collapse multiple slashes: `//` → `/`
   - Remove redundant `./` segments
   - Remove trailing slashes (except root)
3. **Security Validation**:
   - Reject paths containing `..` that would escape the sandbox
   - Reject absolute paths (starting with `/`)
   - Reject null bytes and control characters

**Examples**:
| Input Path | Normalized | Valid? |
|------------|------------|--------|
| `docs/readme.md` | `docs/readme.md` | Yes |
| `./docs//readme.md` | `docs/readme.md` | Yes |
| `docs/../config.md` | `config.md` | Yes |
| `../etc/passwd` | - | No (PATH_TRAVERSAL) |
| `/etc/passwd` | - | No (PATH_TRAVERSAL) |
| `docs/./../..` | - | No (PATH_TRAVERSAL) |

---

## Error Response Format

All errors follow a consistent envelope format:

```json
{
  "error": {
    "code": "MACHINE_READABLE_CODE",
    "message": "Human-readable description"
  }
}
```

### Error Codes

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `PATH_TRAVERSAL` | 403 Forbidden | Path attempts to escape sandbox or is absolute |
| `NOT_FOUND` | 404 Not Found | File does not exist at specified path |
| `IS_DIRECTORY` | 400 Bad Request | Path points to a directory, not a file |
| `INVALID_REVISION` | 400 Bad Request | Revision is not a valid positive integer |
| `CONFLICT` | 409 Conflict | `base_rev` does not match current revision (see Conflict Response) |
| `READ_ERROR` | 500 Internal Server Error | File read operation failed |
| `WRITE_ERROR` | 500 Internal Server Error | File write operation failed |

---

## Revision Semantics

The revision system provides optimistic concurrency control:

1. **Monotonic Counter**: Revision is a `u64` that increments on each successful save
2. **Initial Revision**: New files start at revision `1` upon first save
3. **Optimistic Locking**: Save requests must include `base_rev` matching the current revision
4. **Conflict Detection**: If `base_rev ≠ current_revision`, a 409 Conflict is returned

**Revision Flow**:
```
Client opens file "doc.md" → Receives revision 5
Client edits and saves with base_rev: 5 → Server accepts, returns revision 6
Another client saves with base_rev: 5 → Server rejects with 409 Conflict
```

---

## Endpoints

### 1. Open Document

**POST** `/writer/open`

Open a document for editing. Returns the current content and revision.

#### Request Body

```json
{
  "path": "relative/path/to/file.md"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | File path relative to sandbox |

#### Response Schema (200 OK)

```json
{
  "path": "relative/path/to/file.md",
  "content": "# Document Title\n\nContent here...",
  "mime": "text/markdown",
  "revision": 123,
  "readonly": false
}
```

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Normalized path relative to sandbox |
| `content` | string | Full file content as UTF-8 string |
| `mime` | string | MIME type (e.g., `text/markdown`, `text/plain`) |
| `revision` | integer | Current revision number (u64) |
| `readonly` | boolean | Whether the file is read-only |

#### Example Request

```bash
curl -X POST "http://localhost:8080/writer/open" \
  -H "Content-Type: application/json" \
  -d '{"path": "docs/readme.md"}'
```

#### Example Response

```json
{
  "path": "docs/readme.md",
  "content": "# ChoirOS\n\nA Rust-based multi-agent operating system.",
  "mime": "text/markdown",
  "revision": 42,
  "readonly": false
}
```

#### Errors

- `PATH_TRAVERSAL` (403): Path contains `..` or is absolute
- `NOT_FOUND` (404): File does not exist
- `IS_DIRECTORY` (400): Path points to a directory

---

### 2. Save Document

**POST** `/writer/save`

Save document content with optimistic concurrency control.

#### Request Body

```json
{
  "path": "relative/path/to/file.md",
  "base_rev": 123,
  "content": "# Updated Title\n\nNew content here..."
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | File path relative to sandbox |
| `base_rev` | integer | Yes | Revision this edit was based on |
| `content` | string | Yes | New file content |

#### Response Schema (200 OK)

```json
{
  "path": "relative/path/to/file.md",
  "revision": 124,
  "saved": true
}
```

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Normalized path relative to sandbox |
| `revision` | integer | New revision number after save |
| `saved` | boolean | Always `true` on success |

#### Conflict Response (409 Conflict)

When `base_rev` does not match the current revision:

```json
{
  "error": {
    "code": "CONFLICT",
    "message": "Document was modified by another client"
  },
  "path": "relative/path/to/file.md",
  "current_revision": 125,
  "current_content": "# Other Client's Version\n\nDifferent content..."
}
```

The conflict response includes the current server state so the client can:
1. Show the conflict to the user
2. Perform a three-way merge
3. Retry with the updated `base_rev`

#### Example Request

```bash
curl -X POST "http://localhost:8080/writer/save" \
  -H "Content-Type: application/json" \
  -d '{
    "path": "docs/readme.md",
    "base_rev": 42,
    "content": "# ChoirOS\n\nUpdated description."
  }'
```

#### Example Response (Success)

```json
{
  "path": "docs/readme.md",
  "revision": 43,
  "saved": true
}
```

#### Example Response (Conflict)

```json
{
  "error": {
    "code": "CONFLICT",
    "message": "Document was modified by another client"
  },
  "path": "docs/readme.md",
  "current_revision": 45,
  "current_content": "# ChoirOS\n\nSomeone else edited this."
}
```

#### Errors

- `PATH_TRAVERSAL` (403): Path contains `..` or is absolute
- `NOT_FOUND` (404): File does not exist
- `INVALID_REVISION` (400): `base_rev` is not a valid positive integer
- `IS_DIRECTORY` (400): Path points to a directory
- `CONFLICT` (409): `base_rev` does not match current revision
- `WRITE_ERROR` (500): File write operation failed

---

### 3. Preview Markdown

**POST** `/writer/preview`

Render markdown to HTML for preview purposes. Accepts either a file path or raw content.

#### Request Body

**Option A: Preview by path**
```json
{
  "path": "relative/path/to/file.md"
}
```

**Option B: Preview raw content**
```json
{
  "content": "# Markdown Title\n\nSome **bold** text."
}
```

**Option C: Both (content takes precedence for rendering, path for context)**
```json
{
  "path": "docs/readme.md",
  "content": "# Draft\n\nUnsaved changes..."
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | No* | File path to read and render |
| `content` | string | No* | Raw markdown content to render |

*At least one of `path` or `content` must be provided.

#### Response Schema (200 OK)

```json
{
  "html": "<h1>Markdown Title</h1>\n<p>Some <strong>bold</strong> text.</p>"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `html` | string | Rendered HTML output |

#### Markdown Features

The preview endpoint supports:
- Standard CommonMark syntax
- GitHub-flavored markdown (tables, strikethrough, task lists)
- Fenced code blocks with language hints
- Auto-linked URLs

**Security Note**: The HTML output is raw rendered markdown. The frontend should sanitize if displaying in a security-sensitive context.

#### Example Request

```bash
curl -X POST "http://localhost:8080/writer/preview" \
  -H "Content-Type: application/json" \
  -d '{"content": "# Hello\n\nThis is **bold**."}'
```

#### Example Response

```json
{
  "html": "<h1>Hello</h1>\n<p>This is <strong>bold</strong>.</p>"
}
```

#### Errors

- `PATH_TRAVERSAL` (403): Path contains `..` or is absolute (if path provided)
- `NOT_FOUND` (404): File does not exist (if path provided)
- `IS_DIRECTORY` (400): Path points to a directory (if path provided)
- `READ_ERROR` (500): File read operation failed (if path provided)

---

## Type Definitions (Rust)

### Request Types

```rust
use serde::{Deserialize, Serialize};

/// Request to open a document
#[derive(Debug, Deserialize)]
pub struct OpenDocumentRequest {
    pub path: String,
}

/// Request to save a document
#[derive(Debug, Deserialize)]
pub struct SaveDocumentRequest {
    pub path: String,
    pub base_rev: u64,
    pub content: String,
}

/// Request to preview markdown
#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub path: Option<String>,
    pub content: Option<String>,
}
```

### Response Types

```rust
/// Response for successful document open
#[derive(Debug, Serialize)]
pub struct OpenDocumentResponse {
    pub path: String,
    pub content: String,
    pub mime: String,
    pub revision: u64,
    pub readonly: bool,
}

/// Response for successful document save
#[derive(Debug, Serialize)]
pub struct SaveDocumentResponse {
    pub path: String,
    pub revision: u64,
    pub saved: bool,
}

/// Response for markdown preview
#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    pub html: String,
}
```

### Error Types

```rust
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;
use thiserror::Error;

/// Writer-specific error codes
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

/// Standard error response body
#[derive(Debug, Serialize)]
pub struct WriterErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct WriterErrorResponse {
    pub error: WriterErrorDetail,
}

/// Conflict response includes current server state
#[derive(Debug, Serialize)]
pub struct ConflictResponse {
    #[serde(flatten)]
    pub error: WriterErrorResponse,
    pub path: String,
    pub current_revision: u64,
    pub current_content: String,
}
```

---

## Security / Sandbox Requirements

### Path Validation

All file operations must use the `validate_path()` pattern from `files.rs`:

```rust
/// Validates and normalizes a path relative to sandbox
fn validate_path(sandbox: &Path, user_path: &str) -> Result<PathBuf, axum::response::Response> {
    // Reject null bytes
    if user_path.contains('\0') {
        return Err(writer_error(
            WriterErrorCode::PathTraversal,
            "Path contains null bytes"
        ).into_response());
    }

    // Reject absolute paths
    if user_path.starts_with('/') {
        return Err(writer_error(
            WriterErrorCode::PathTraversal,
            "Absolute paths are not allowed"
        ).into_response());
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
                        "Path escapes sandbox directory"
                    ).into_response());
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(writer_error(
                    WriterErrorCode::PathTraversal,
                    "Path contains invalid components"
                ).into_response());
            }
        }
    }

    // Join with sandbox and validate
    let full_path = sandbox.join(&normalized);
    Ok(full_path)
}
```

### Sandbox Root

```rust
/// Sandbox root path - all file operations are constrained to this directory
fn sandbox_root() -> PathBuf {
    Path::new("/Users/wiz/choiros-rs/sandbox").to_path_buf()
}
```

### Security Checklist

- [ ] All paths validated through `validate_path()` before use
- [ ] Path traversal attempts (`..`) rejected with 403
- [ ] Absolute paths rejected with 403
- [ ] Final path checked to be within sandbox root

---

## Example Flows

### Flow 1: Successful Edit and Save

```bash
# 1. Client opens document
curl -X POST "http://localhost:8080/writer/open" \
  -d '{"path": "docs/guide.md"}'

# Response:
# {
#   "path": "docs/guide.md",
#   "content": "# Guide\n\nInitial content.",
#   "revision": 10,
#   "readonly": false
# }

# 2. Client edits and saves
curl -X POST "http://localhost:8080/writer/save" \
  -d '{
    "path": "docs/guide.md",
    "base_rev": 10,
    "content": "# Guide\n\nUpdated content."
  }'

# Response:
# {
#   "path": "docs/guide.md",
#   "revision": 11,
#   "saved": true
# }
```

### Flow 2: Conflict Resolution

```bash
# Client A and Client B both open revision 10
# Client A saves first:

curl -X POST "http://localhost:8080/writer/save" \
  -d '{
    "path": "docs/guide.md",
    "base_rev": 10,
    "content": "Client A version"
  }'
# Success: revision 11

# Client B tries to save (still using base_rev 10):
curl -X POST "http://localhost:8080/writer/save" \
  -d '{
    "path": "docs/guide.md",
    "base_rev": 10,
    "content": "Client B version"
  }'

# Response (409 Conflict):
# {
#   "error": {
#     "code": "CONFLICT",
#     "message": "Document was modified by another client"
#   },
#   "path": "docs/guide.md",
#   "current_revision": 11,
#   "current_content": "Client A version"
# }

# Client B must:
# 1. Show conflict to user or auto-merge
# 2. Retry with base_rev: 11
```

### Flow 3: Preview Unsaved Changes

```bash
# Preview current file content
curl -X POST "http://localhost:8080/writer/preview" \
  -d '{"path": "docs/guide.md"}'

# Preview unsaved draft content
curl -X POST "http://localhost:8080/writer/preview" \
  -d '{"content": "# Draft\n\nWork in progress..."}'
```

---

## Implementation Notes

### Dependencies

Add to `sandbox/Cargo.toml`:

```toml
[dependencies]
pulldown-cmark = "0.12"  # For markdown rendering
```

### Route Registration

Add to `sandbox/src/api/mod.rs`:

```rust
pub mod writer;

// In router() function:
.route("/writer/open", post(writer::open_document))
.route("/writer/save", post(writer::save_document))
.route("/writer/preview", post(writer::preview_markdown))
```

### Revision Storage

Revisions are stored in SQLite using the existing database connection:

```sql
CREATE TABLE IF NOT EXISTS document_revisions (
    path TEXT PRIMARY KEY,
    revision INTEGER NOT NULL DEFAULT 1,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2024-02-09 | Initial API contract |
