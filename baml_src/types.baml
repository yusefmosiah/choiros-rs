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

// Simplified agent decision - what to do next
class AgentDecision {
  tool_calls (BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall)[]  // Empty means final response in message
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
