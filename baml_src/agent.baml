// ChoirOS Chat Agent Functions
// BAML-powered agent functions for planning and response synthesis

function Decide(
  messages: Message[],
  context: string,
  available_tools: string
) -> AgentDecision {
  client Orchestrator
  prompt #"
    You are ChoirOS, an intelligent AI assistant.

    {{ context }}

    Available Tools:
    {{ available_tools }}

    Conversation:
    {{ messages }}

    Decide what to do next:
    - Use action: ToolCall with appropriate tool_calls to execute tools
    - Use action: Complete with message when you have the answer
    - Use action: Block with message if you cannot proceed

    Tool-use guidelines:
    - For external, time-sensitive, or verifiable requests, use tools instead of guessing
    - Resolve relative time references against the current context
    - Call multiple tools in parallel when independent
    - For each tool call, use exactly one schema variant matching the tool_name
    - Include only valid fields for that tool's args and omit absent optional fields
    - Never emit placeholder keys with null values
    - Always set message:
      - ToolCall: brief plan for next execution
      - Complete: direct final answer
      - Block: why you are blocked
    - Once you have sufficient evidence, use Complete with a direct answer in message

    {{ ctx.output_format(prefix="Return only valid JSON.\n", or_splitter=" | ", hoist_classes=true) }}
  "#
}

// Quick response for simple queries (no tool use)
function QuickResponse(
  user_message: string,
  conversation_history: string
) -> string {
  client FastResponse
  prompt #"
    You are ChoirOS, a helpful AI assistant.

    Conversation History:
    {{ conversation_history }}

    User: {{ user_message }}

    Provide a brief, helpful response.
  "#
}
