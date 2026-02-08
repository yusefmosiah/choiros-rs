// ChoirOS Chat Agent Types
// Core types for BAML-powered chat agent with tool execution

// Message in conversation
class Message {
  role string
  content string
}

class BashToolArgs {
  command string?
  cmd string?
  cwd string?
  reasoning string?
  timeout_ms int?
  model string?
}

class ReadFileToolArgs {
  path string?
  limit int?
  offset int?
}

class WriteFileToolArgs {
  path string?
  content string?
}

class ListFilesToolArgs {
  path string?
  recursive bool?
}

class SearchFilesToolArgs {
  pattern string?
  path string?
  file_pattern string?
}

// Individual tool call from the agent
class AgentToolArgs {
  bash BashToolArgs?
  read_file ReadFileToolArgs?
  write_file WriteFileToolArgs?
  list_files ListFilesToolArgs?
  search_files SearchFilesToolArgs?

  // Legacy flat fields kept for compatibility with models that don't yet
  // follow the nested per-tool schema.
  command string?
  cmd string?
  cwd string?
  reasoning string?
  timeout_ms int?
  model string?
  path string?
  content string?
  pattern string?
  file_pattern string?
  recursive bool?
  limit int?
  offset int?
}

class AgentToolCall {
  tool_name string
  tool_args AgentToolArgs
  reasoning string?
}

// The agent's plan - thinking, tool calls, and final response
class AgentPlan {
  thinking string
  tool_calls AgentToolCall[]
  final_response string?
  confidence float
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
