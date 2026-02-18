// ChoirOS Chat Agent Types
// Core types for BAML-powered chat agent with tool execution

// Message in conversation
class Message {
  role string
  content string
}

class BashToolArgs {
  command string
}

class WebSearchToolArgs {
  query string
}

class FetchUrlToolArgs {
  path string
}

class FileReadToolArgs {
  path string
}

class FileWriteToolArgs {
  path string
  content string
}

class FileEditToolArgs {
  path string
  old_text string
  new_text string
}

class MessageWriterToolArgs {
  content string
  mode string
  path string?
  mode_arg string?
}

class FinishedToolArgs {
  summary string?
}

class BashToolCall {
  tool_name "bash"
  tool_args BashToolArgs
  reasoning string?
}

class WebSearchToolCall {
  tool_name "web_search"
  tool_args WebSearchToolArgs
  reasoning string?
}

class FetchUrlToolCall {
  tool_name "fetch_url"
  tool_args FetchUrlToolArgs
  reasoning string?
}

class FileReadToolCall {
  tool_name "file_read"
  tool_args FileReadToolArgs
  reasoning string?
}

class FileWriteToolCall {
  tool_name "file_write"
  tool_args FileWriteToolArgs
  reasoning string?
}

class FileEditToolCall {
  tool_name "file_edit"
  tool_args FileEditToolArgs
  reasoning string?
}

class MessageWriterToolCall {
  tool_name "message_writer"
  tool_args MessageWriterToolArgs
  reasoning string?
}

class FinishedToolCall {
  tool_name "finished"
  tool_args FinishedToolArgs
  reasoning string?
}

// Simplified agent decision - what to do next
class AgentDecision {
  tool_calls (BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall)[]  // Completion requires a finished tool call
  message string
}

// Result of executing a tool
class ToolResult {
  tool_name string
  success bool
  output string
  error string?
}

// Streaming chunk types for WebSocket
class StreamChunk {
  chunk_type string // "thinking", "tool_call", "tool_result", "response", "error"
  content string
}

// ---------------------------------------------------------------------------
// Phase 2.2 — Citation Types
// ---------------------------------------------------------------------------

/// Why a resource was cited by a researcher or writer.
enum CitationKind {
  /// Researcher retrieved the resource and pulled it into context.
  RetrievedContext
  /// Appears as a link or inline reference in document text.
  InlineReference
  /// This run extends or revises the cited artifact.
  BuildsOn
  /// Explicitly disputes a prior artifact.
  Contradicts
  /// Restates a prior objective or directive.
  Reissues
}

/// A single citation proposed by the researcher or confirmed/rejected by the writer.
class Citation {
  /// Artifact path, version_id, input_id, URL, or block_id.
  cited_id string
  cite_kind CitationKind
  /// Model confidence in this citation [0.0, 1.0].
  confidence float
  /// Specific text span that triggered this citation, if extractable.
  excerpt string?
  /// Why this citation was relevant.
  rationale string
}
