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
    - If more evidence or actions are needed, return one or more tool_calls
    - If you have enough information, include a `finished` tool call and put the final answer in message

    Tool-use guidelines:
    - For external, time-sensitive, or verifiable requests, use tools instead of guessing
    - Resolve relative time references against the current context
    - Call multiple tools in parallel when independent
    - If `message_writer` is available, use it for actor-to-actor updates/completion before calling `finished`
    - For each tool call, use exactly one schema variant matching the tool_name
    - Include only valid fields for that tool's args and omit absent optional fields
    - Never emit placeholder keys with null values
    - Always set message:
      - With tool calls: brief execution plan for this step
      - With `finished`: direct final answer
    - Once you have sufficient evidence, call `finished` and provide the final answer in message

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
