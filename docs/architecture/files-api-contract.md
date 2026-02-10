# Files App API Contract

## Overview

The Files App API provides secure file system operations within the ChoirOS sandbox directory. All paths are relative to the sandbox root (`/Users/wiz/choiros-rs/sandbox`) and path traversal attempts are rejected.

**Base URL**: `/api/files`

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
| `src/main.rs` | `src/main.rs` | Yes |
| `./src//main.rs` | `src/main.rs` | Yes |
| `src/../config.toml` | `config.toml` | Yes |
| `../etc/passwd` | - | No (PATH_TRAVERSAL) |
| `/etc/passwd` | - | No (PATH_TRAVERSAL) |
| `src/./../..` | - | No (PATH_TRAVERSAL) |

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
| `NOT_FOUND` | 404 Not Found | File or directory does not exist |
| `NOT_A_FILE` | 400 Bad Request | Path exists but is not a file (e.g., trying to read a directory) |
| `NOT_A_DIRECTORY` | 400 Bad Request | Path exists but is not a directory (e.g., trying to list a file) |
| `ALREADY_EXISTS` | 409 Conflict | File or directory already exists at target path |
| `PERMISSION_DENIED` | 403 Forbidden | Insufficient permissions for operation |
| `INVALID_CONTENT` | 400 Bad Request | Content is invalid (e.g., binary when text expected) |
| `INTERNAL_ERROR` | 500 Internal Server Error | Unexpected server error |

---

## Endpoints

### 1. List Directory Contents

**GET** `/api/files/list`

List files and directories within a directory.

#### Query Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | No | Directory path relative to sandbox (default: `""` for root) |
| `recursive` | boolean | No | Include subdirectories recursively (default: `false`) |

#### Response Schema

```json
{
  "path": "relative/path",
  "entries": [
    {
      "name": "filename",
      "path": "relative/path/to/file",
      "is_file": true,
      "is_dir": false,
      "size": 1234,
      "modified_at": "2024-01-15T10:30:00Z"
    }
  ],
  "total_count": 42
}
```

#### Example Request

```bash
curl "http://localhost:8080/api/files/list?path=src&recursive=false"
```

#### Example Response

```json
{
  "path": "src",
  "entries": [
    {
      "name": "main.rs",
      "path": "src/main.rs",
      "is_file": true,
      "is_dir": false,
      "size": 2048,
      "modified_at": "2024-01-15T10:30:00Z"
    },
    {
      "name": "lib.rs",
      "path": "src/lib.rs",
      "is_file": true,
      "is_dir": false,
      "size": 1024,
      "modified_at": "2024-01-15T09:15:00Z"
    },
    {
      "name": "utils",
      "path": "src/utils",
      "is_file": false,
      "is_dir": true,
      "size": 0,
      "modified_at": "2024-01-14T16:45:00Z"
    }
  ],
  "total_count": 3
}
```

---

### 2. Get File/Directory Metadata

**GET** `/api/files/metadata`

Retrieve metadata for a file or directory.

#### Query Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Path relative to sandbox |

#### Response Schema

```json
{
  "name": "filename",
  "path": "relative/path",
  "is_file": true,
  "is_dir": false,
  "size": 1234,
  "created_at": "2024-01-15T10:30:00Z",
  "modified_at": "2024-01-15T10:30:00Z",
  "permissions": "644"
}
```

#### Example Request

```bash
curl "http://localhost:8080/api/files/metadata?path=src/main.rs"
```

#### Example Response

```json
{
  "name": "main.rs",
  "path": "src/main.rs",
  "is_file": true,
  "is_dir": false,
  "size": 2048,
  "created_at": "2024-01-15T10:30:00Z",
  "modified_at": "2024-01-15T10:30:00Z",
  "permissions": "644"
}
```

---

### 3. Read File Content

**GET** `/api/files/content`

Read the content of a text file. Binary files are not supported.

#### Query Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | File path relative to sandbox |
| `offset` | integer | No | Byte offset to start reading (default: `0`) |
| `limit` | integer | No | Maximum bytes to read (default: `1048576` - 1MB max) |

#### Response Schema

```json
{
  "path": "relative/path",
  "content": "file content as string",
  "size": 1234,
  "is_truncated": false,
  "encoding": "utf-8"
}
```

#### Example Request

```bash
curl "http://localhost:8080/api/files/content?path=src/main.rs"
```

#### Example Response

```json
{
  "path": "src/main.rs",
  "content": "fn main() {\n    println!(\"Hello, world!\");\n}",
  "size": 42,
  "is_truncated": false,
  "encoding": "utf-8"
}
```

#### Notes

- Content is returned as a UTF-8 string
- Binary files (detected by null bytes or invalid UTF-8) return `INVALID_CONTENT` error
- Large files are truncated at 1MB by default; use `offset` and `limit` for pagination

---

### 4. Create File

**POST** `/api/files/create`

Create a new empty file or file with initial content.

#### Request Body

```json
{
  "path": "relative/path/to/file.txt",
  "content": "optional initial content",
  "overwrite": false
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | File path relative to sandbox |
| `content` | string | No | Initial file content (default: empty) |
| `overwrite` | boolean | No | Whether to overwrite existing file (default: `false`) |

#### Response Schema

```json
{
  "path": "relative/path/to/file.txt",
  "created": true,
  "size": 0
}
```

#### Example Request

```bash
curl -X POST "http://localhost:8080/api/files/create" \
  -H "Content-Type: application/json" \
  -d '{"path": "config/app.toml", "content": "[app]\nname = \"MyApp\""}'
```

#### Example Response

```json
{
  "path": "config/app.toml",
  "created": true,
  "size": 22
}
```

#### Errors

- `ALREADY_EXISTS` if file exists and `overwrite` is `false`
- `NOT_A_DIRECTORY` if parent path is not a directory

---

### 5. Write File Content

**POST** `/api/files/write`

Write or overwrite file content.

#### Request Body

```json
{
  "path": "relative/path/to/file.txt",
  "content": "new file content",
  "append": false,
  "create_if_missing": true
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | File path relative to sandbox |
| `content` | string | Yes | Content to write |
| `append` | boolean | No | Append to existing content (default: `false`) |
| `create_if_missing` | boolean | No | Create file if it does not exist (default: `true`) |

#### Response Schema

```json
{
  "path": "relative/path/to/file.txt",
  "bytes_written": 16,
  "size": 32
}
```

#### Example Request

```bash
curl -X POST "http://localhost:8080/api/files/write" \
  -H "Content-Type: application/json" \
  -d '{"path": "log.txt", "content": "New log entry\n", "append": true}'
```

#### Example Response

```json
{
  "path": "log.txt",
  "bytes_written": 14,
  "size": 1024
}
```

#### Errors

- `NOT_FOUND` if file does not exist and `create_if_missing` is `false`
- `NOT_A_FILE` if path exists but is a directory

---

### 6. Create Directory

**POST** `/api/files/mkdir`

Create a new directory (and parent directories if needed).

#### Request Body

```json
{
  "path": "relative/path/to/new/dir",
  "recursive": true
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Directory path relative to sandbox |
| `recursive` | boolean | No | Create parent directories if needed (default: `true`) |

#### Response Schema

```json
{
  "path": "relative/path/to/new/dir",
  "created": true
}
```

#### Example Request

```bash
curl -X POST "http://localhost:8080/api/files/mkdir" \
  -H "Content-Type: application/json" \
  -d '{"path": "src/components/widgets", "recursive": true}'
```

#### Example Response

```json
{
  "path": "src/components/widgets",
  "created": true
}
```

#### Errors

- `ALREADY_EXISTS` if directory already exists
- `NOT_A_DIRECTORY` if a parent path component exists but is not a directory

---

### 7. Rename/Move File or Directory

**POST** `/api/files/rename`

Rename or move a file or directory.

#### Request Body

```json
{
  "source": "relative/path/to/source",
  "target": "relative/path/to/target",
  "overwrite": false
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `source` | string | Yes | Source path relative to sandbox |
| `target` | string | Yes | Target path relative to sandbox |
| `overwrite` | boolean | No | Overwrite target if it exists (default: `false`) |

#### Response Schema

```json
{
  "source": "relative/path/to/source",
  "target": "relative/path/to/target",
  "renamed": true
}
```

#### Example Request

```bash
curl -X POST "http://localhost:8080/api/files/rename" \
  -H "Content-Type: application/json" \
  -d '{"source": "old_name.txt", "target": "new_name.txt"}'
```

#### Example Response

```json
{
  "source": "old_name.txt",
  "target": "new_name.txt",
  "renamed": true
}
```

#### Errors

- `NOT_FOUND` if source does not exist
- `ALREADY_EXISTS` if target exists and `overwrite` is `false`
- `NOT_A_DIRECTORY` if target parent is not a directory

---

### 8. Delete File or Directory

**POST** `/api/files/delete`

Delete a file or directory.

#### Request Body

```json
{
  "path": "relative/path/to/delete",
  "recursive": false
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Path to delete relative to sandbox |
| `recursive` | boolean | No | Delete directories recursively (default: `false`) |

#### Response Schema

```json
{
  "path": "relative/path/to/delete",
  "deleted": true,
  "type": "file"
}
```

#### Example Request

```bash
curl -X POST "http://localhost:8080/api/files/delete" \
  -H "Content-Type: application/json" \
  -d '{"path": "temp/old_file.txt"}'
```

#### Example Response

```json
{
  "path": "temp/old_file.txt",
  "deleted": true,
  "type": "file"
}
```

#### Errors

- `NOT_FOUND` if path does not exist
- `NOT_A_FILE` if path is a non-empty directory and `recursive` is `false`

---

### 9. Copy File

**POST** `/api/files/copy`

Copy a file to a new location.

#### Request Body

```json
{
  "source": "relative/path/to/source",
  "target": "relative/path/to/target",
  "overwrite": false
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `source` | string | Yes | Source file path relative to sandbox |
| `target` | string | Yes | Target file path relative to sandbox |
| `overwrite` | boolean | No | Overwrite target if it exists (default: `false`) |

#### Response Schema

```json
{
  "source": "relative/path/to/source",
  "target": "relative/path/to/target",
  "copied": true,
  "size": 1234
}
```

#### Example Request

```bash
curl -X POST "http://localhost:8080/api/files/copy" \
  -H "Content-Type: application/json" \
  -d '{"source": "config/template.toml", "target": "config/active.toml"}'
```

#### Example Response

```json
{
  "source": "config/template.toml",
  "target": "config/active.toml",
  "copied": true,
  "size": 512
}
```

#### Errors

- `NOT_FOUND` if source does not exist
- `NOT_A_FILE` if source is not a file
- `ALREADY_EXISTS` if target exists and `overwrite` is `false`

---

## Common Data Types

### DirectoryEntry

```json
{
  "name": "string",
  "path": "string",
  "is_file": "boolean",
  "is_dir": "boolean",
  "size": "integer (bytes)",
  "modified_at": "ISO 8601 timestamp"
}
```

### FileMetadata

```json
{
  "name": "string",
  "path": "string",
  "is_file": "boolean",
  "is_dir": "boolean",
  "size": "integer (bytes)",
  "created_at": "ISO 8601 timestamp",
  "modified_at": "ISO 8601 timestamp",
  "permissions": "string (Unix mode)"
}
```

---

## Implementation Notes

### Security Considerations

1. **Path Validation**: All paths must be validated before any filesystem operation
2. **Sandbox Enforcement**: Use `canonicalize()` and prefix check to ensure paths stay within sandbox
3. **Symlinks**: Symlinks that escape the sandbox should be rejected or followed with validation
4. **Resource Limits**: Implement reasonable limits on file sizes and request frequencies

### Performance Considerations

1. **Large Files**: Use streaming for large file reads/writes
2. **Directory Listing**: Consider pagination for large directories
3. **Caching**: Metadata responses can be cached with short TTL

### Rust Implementation Sketch

```rust
/// Validates and normalizes a path relative to sandbox
fn validate_path(sandbox: &Path, user_path: &str) -> Result<PathBuf, FileError> {
    // Reject absolute paths
    if user_path.starts_with('/') {
        return Err(FileError::PathTraversal);
    }

    // Reject null bytes
    if user_path.contains('\0') {
        return Err(FileError::PathTraversal);
    }

    // Normalize path
    let normalized = Path::new(user_path)
        .components()
        .fold(PathBuf::new(), |mut acc, comp| {
            match comp {
                Component::Normal(s) => acc.push(s),
                Component::CurDir => {}, // skip .
                Component::ParentDir => { acc.pop(); }
                _ => {}
            }
            acc
        });

    // Join with sandbox and canonicalize
    let full_path = sandbox.join(&normalized);
    let canonical = full_path.canonicalize()
        .or_else(|_| Ok::<_, FileError>(full_path.clone()))?;

    // Ensure still within sandbox
    if !canonical.starts_with(sandbox) {
        return Err(FileError::PathTraversal);
    }

    Ok(canonical)
}
```

---

## Changelog

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2024-01-15 | Initial API contract |
