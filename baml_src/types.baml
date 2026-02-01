// ChoirOS Chat Agent Types
// Core types for BAML-powered chat agent with tool execution

// Message in conversation
class Message {
  role string
  content string
}

// Individual tool call from the agent
class AgentToolCall {
  tool_name string
  tool_args string
  reasoning string
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
