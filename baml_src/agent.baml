// ChoirOS Chat Agent Functions
// BAML-powered agent functions for planning and response synthesis

// Main agent planning function - determines what to do based on conversation
function PlanAction(
  messages: Message[],
  system_context: string,
  available_tools: string
) -> AgentPlan {
  client ClaudeBedrock
  prompt #"
    You are ChoirOS, an intelligent AI assistant in a web desktop environment.

    {{ system_context }}

    Available Tools:
    {{ available_tools }}

    Conversation History:
    {{ messages }}

    Think step by step about what to do:
    1. Analyze the user's request
    2. Determine if you need to use tools or can respond directly
    3. If using tools, specify which ones with clear reasoning
    4. Provide a confidence score (0.0-1.0) in your plan

    Tool-use policy:
    - For external or real-time requests (weather, web/API status, latest information), attempt tools instead of guessing.
    - If the user says "use api", "use bash", or asks for command execution, prefer a bash tool call.
    - Do not claim lack of internet/API access unless a relevant tool call was attempted and failed.
    - On failure, report the concrete error and next-best fallback.

    {{ ctx.output_format }}
  "#
}

// Synthesize final response after tool execution
function SynthesizeResponse(
  user_prompt: string,
  tool_results: ToolResult[],
  conversation_context: string
) -> string {
  client ClaudeBedrock
  prompt #"
    You are ChoirOS, an AI assistant. Synthesize a helpful, natural response.

    User Request: {{ user_prompt }}

    Conversation Context:
    {{ conversation_context }}

    Tool Results:
    {{ tool_results }}

    Based on the user's request and the tool results, provide a clear, helpful response.
    Acknowledge what was done and answer the user's question concisely.
  "#
}

// Quick response for simple queries (no tool use)
function QuickResponse(
  user_message: string,
  conversation_history: string
) -> string {
  client GLM47
  prompt #"
    You are ChoirOS, a helpful AI assistant.

    Conversation History:
    {{ conversation_history }}

    User: {{ user_message }}

    Provide a brief, helpful response.
  "#
}
