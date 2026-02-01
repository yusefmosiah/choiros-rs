//! Tool registry for ChoirOS Chat Agent
//!
//! Provides extensible tool system for agent to execute commands,
//! read/write files, and interact with the system.

use serde_json::Value;
use std::collections::HashMap;
use thiserror::Error;

/// Tool registry containing all available tools
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

/// Trait for tools that can be executed by the agent
pub trait Tool: Send + Sync {
    /// Tool name (must be unique)
    fn name(&self) -> &str;

    /// Human-readable description for LLM
    fn description(&self) -> &str;

    /// JSON Schema for tool parameters
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with given arguments
    fn execute(&self, args: Value) -> Result<ToolOutput, ToolError>;
}

/// Output from tool execution
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub success: bool,
    pub content: String,
}

/// Tool execution error
#[derive(Debug, Error, Clone)]
#[error("Tool error: {message}")]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl ToolRegistry {
    /// Create a new tool registry with default tools
    pub fn new() -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();

        // Register default tools
        tools.insert("bash".to_string(), Box::new(BashTool) as Box<dyn Tool>);
        tools.insert(
            "read_file".to_string(),
            Box::new(ReadFileTool) as Box<dyn Tool>,
        );
        tools.insert(
            "write_file".to_string(),
            Box::new(WriteFileTool) as Box<dyn Tool>,
        );
        tools.insert(
            "list_files".to_string(),
            Box::new(ListFilesTool) as Box<dyn Tool>,
        );
        tools.insert(
            "search_files".to_string(),
            Box::new(SearchFilesTool) as Box<dyn Tool>,
        );

        Self { tools }
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&Box<dyn Tool>> {
        self.tools.get(name)
    }

    /// Execute a tool by name with arguments
    pub fn execute(&self, name: &str, args: Value) -> Result<ToolOutput, ToolError> {
        match self.tools.get(name) {
            Some(tool) => tool.execute(args),
            None => Err(ToolError::new(format!("Tool '{}' not found", name))),
        }
    }

    /// Get formatted descriptions of all tools for BAML prompt
    pub fn descriptions(&self) -> String {
        self.tools
            .values()
            .map(|t| {
                let schema = serde_json::to_string(&t.parameters_schema()).unwrap_or_default();
                format!(
                    "Tool: {}\nDescription: {}\nParameters Schema: {}\n",
                    t.name(),
                    t.description(),
                    schema
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get list of available tool names
    pub fn available_tools(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tool Implementations
// ============================================================================

/// Bash tool - executes shell commands
pub struct BashTool;

impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute shell commands. Use for file operations, system commands, git operations, etc."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 30000)",
                    "default": 30000
                }
            },
            "required": ["command"]
        })
    }

    fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("Missing 'command' parameter"))?
            .to_string();

        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(30000);

        // Execute with timeout using tokio::process::Command
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|e| ToolError::new(format!("No async runtime: {}", e)))?;

        let output = runtime.block_on(async {
            tokio::time::timeout(
                tokio::time::Duration::from_millis(timeout_ms),
                tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&command)
                    .output(),
            )
            .await
        });

        match output {
            Ok(Ok(result)) => {
                let stdout = String::from_utf8_lossy(&result.stdout).to_string();
                let stderr = String::from_utf8_lossy(&result.stderr).to_string();

                if result.status.success() {
                    Ok(ToolOutput {
                        success: true,
                        content: stdout,
                    })
                } else {
                    Ok(ToolOutput {
                        success: false,
                        content: format!(
                            "Exit code: {:?}\nStdout: {}\nStderr: {}",
                            result.status.code(),
                            stdout,
                            stderr
                        ),
                    })
                }
            }
            Ok(Err(e)) => Err(ToolError::new(format!("Failed to execute: {}", e))),
            Err(_) => Err(ToolError::new(format!(
                "Command timed out after {}ms",
                timeout_ms
            ))),
        }
    }
}

/// Read file tool - reads file contents
pub struct ReadFileTool;

impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Use for viewing code, logs, configuration files, etc."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of lines to read (0 = all)",
                    "default": 0
                },
                "offset": {
                    "type": "integer",
                    "description": "Line offset to start reading from (0-based)",
                    "default": 0
                }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("Missing 'path' parameter"))?;

        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

        // Security: Only allow reading within the project directory
        let path_buf = std::path::PathBuf::from(path);
        if path_buf.is_absolute() && !path_buf.starts_with("/Users/wiz/choiros-rs") {
            return Err(ToolError::new(
                "Cannot read files outside project directory",
            ));
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| ToolError::new(format!("Failed to read file: {}", e)))?;

        let lines: Vec<&str> = content.lines().collect();
        let start = offset.min(lines.len());
        let end = if limit == 0 {
            lines.len()
        } else {
            (start + limit).min(lines.len())
        };

        let result = lines[start..end].join("\n");

        Ok(ToolOutput {
            success: true,
            content: result,
        })
    }
}

/// Write file tool - writes content to a file
pub struct WriteFileTool;

impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("Missing 'path' parameter"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("Missing 'content' parameter"))?
            .to_string();

        // Security: Only allow writing within the project directory
        let path_buf = std::path::PathBuf::from(path);
        if path_buf.is_absolute() && !path_buf.starts_with("/Users/wiz/choiros-rs") {
            return Err(ToolError::new(
                "Cannot write files outside project directory",
            ));
        }

        // Create parent directories if needed
        if let Some(parent) = path_buf.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ToolError::new(format!("Failed to create directory: {}", e)))?;
        }

        std::fs::write(path, content)
            .map_err(|e| ToolError::new(format!("Failed to write file: {}", e)))?;

        Ok(ToolOutput {
            success: true,
            content: format!("Successfully wrote to {}", path),
        })
    }
}

/// List files tool - list directory contents
pub struct ListFilesTool;

impl Tool for ListFilesTool {
    fn name(&self) -> &str {
        "list_files"
    }

    fn description(&self) -> &str {
        "List files and directories in a given path. Use for exploring the project structure."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list (default: current directory)",
                    "default": "."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "List recursively",
                    "default": false
                }
            }
        })
    }

    fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let recursive = args
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let path_buf = std::path::PathBuf::from(path);

        // Security check
        if path_buf.is_absolute() && !path_buf.starts_with("/Users/wiz/choiros-rs") {
            return Err(ToolError::new(
                "Cannot list files outside project directory",
            ));
        }

        let mut files = Vec::new();

        if recursive {
            for entry in walkdir::WalkDir::new(path).max_depth(10) {
                match entry {
                    Ok(e) => {
                        let path = e.path().display().to_string();
                        let file_type = if e.file_type().is_dir() {
                            "dir"
                        } else {
                            "file"
                        };
                        files.push(format!("{}: {}", file_type, path));
                    }
                    Err(e) => files.push(format!("error: {}", e)),
                }
            }
        } else {
            let entries = std::fs::read_dir(path)
                .map_err(|e| ToolError::new(format!("Failed to read directory: {}", e)))?;

            for entry in entries.flatten() {
                let path = entry.path().display().to_string();
                let file_type = if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    "dir"
                } else {
                    "file"
                };
                files.push(format!("{}: {}", file_type, path));
            }
        }

        files.sort();

        Ok(ToolOutput {
            success: true,
            content: files.join("\n"),
        })
    }
}

/// Search files tool - search for text in files
pub struct SearchFilesTool;

impl Tool for SearchFilesTool {
    fn name(&self) -> &str {
        "search_files"
    }

    fn description(&self) -> &str {
        "Search for text patterns in files. Use grep/ripgrep for fast text search."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (default: .)",
                    "default": "."
                },
                "file_pattern": {
                    "type": "string",
                    "description": "File pattern to match (e.g., '*.rs', '*.baml')",
                    "default": "*"
                }
            },
            "required": ["pattern"]
        })
    }

    fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::new("Missing 'pattern' parameter"))?;

        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let file_pattern = args
            .get("file_pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("*");

        // Use ripgrep if available
        let runtime = tokio::runtime::Handle::try_current()
            .map_err(|e| ToolError::new(format!("No async runtime: {}", e)))?;

        let output = runtime.block_on(async {
            tokio::process::Command::new("rg")
                .arg("-n")
                .arg("--type-add")
                .arg(format!(
                    "custom:*.{}",
                    file_pattern.trim_start_matches("*").trim_start_matches(".")
                ))
                .arg("-tcustom")
                .arg(pattern)
                .arg(path)
                .output()
                .await
        });

        match output {
            Ok(result) => {
                let stdout = String::from_utf8_lossy(&result.stdout).to_string();
                let stderr = String::from_utf8_lossy(&result.stderr).to_string();

                Ok(ToolOutput {
                    success: result.status.success(),
                    content: if stdout.is_empty() { stderr } else { stdout },
                })
            }
            Err(e) => Err(ToolError::new(format!("Search failed: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_registry_creation() {
        let registry = ToolRegistry::new();

        // Should have default tools
        assert!(registry.get("bash").is_some());
        assert!(registry.get("read_file").is_some());
        assert!(registry.get("write_file").is_some());
        assert!(registry.get("list_files").is_some());
        assert!(registry.get("search_files").is_some());

        // Non-existent tool should return None
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_read_file_tool_outside_project() {
        let tool = ReadFileTool;
        let args = serde_json::json!({
            "path": "/etc/passwd"
        });

        let result = tool.execute(args);
        assert!(result.is_err());
    }
}
