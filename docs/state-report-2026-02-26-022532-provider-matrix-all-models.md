# Provider Matrix Report (2026-02-26-022532)
Date: 2026-02-26
Kind: Report
Status: Active
Requires: []

## Scope
Full allowed model set from \

## Command
```bash
./scripts/ops/validate-local-provider-matrix.sh --models "ClaudeBedrockOpus46,ClaudeBedrockSonnet46,ClaudeBedrockSonnet45,ClaudeBedrockHaiku45,ZaiGLM5,ZaiGLM47,ZaiGLM47Flash,KimiK25" --skip-gateway-search
```

## Output
```text
== Model provider matrix ==
models=ClaudeBedrockOpus46,ClaudeBedrockSonnet46,ClaudeBedrockSonnet45,ClaudeBedrockHaiku45,ZaiGLM5,ZaiGLM47,ZaiGLM47Flash,KimiK25

running 1 test
2026-02-26T02:25:35.672 [BAML INFO] Function QuickResponse:
    Client: FastResponse (us.anthropic.claude-sonnet-4-6) - 1204ms. StopReason: end_turn. Tokens(in/out): 41/4
    ---PROMPT---
    user: You are ChoirOS, a helpful AI assistant.
    
    Conversation History:
    
    
    User: Reply with exactly: OK
    
    Provide a brief, helpful response.
    
    ---LLM REPLY---
    OK
    ---Parsed Response (string)---
    "OK"
2026-02-26T02:25:36.549 [BAML INFO] Function QuickResponse:
    Client: FastResponse (us.anthropic.claude-opus-4-6-v1) - 2082ms. StopReason: end_turn. Tokens(in/out): 41/4
    ---PROMPT---
    user: You are ChoirOS, a helpful AI assistant.
    
    Conversation History:
    
    
    User: Reply with exactly: OK
    
    Provide a brief, helpful response.
    
    ---LLM REPLY---
    OK
    ---Parsed Response (string)---
    "OK"
2026-02-26T02:25:37.648 [BAML INFO] Function QuickResponse:
    Client: FastResponse (us.anthropic.claude-sonnet-4-5-20250929-v1:0) - 1973ms. StopReason: end_turn. Tokens(in/out): 41/4
    ---PROMPT---
    user: You are ChoirOS, a helpful AI assistant.
    
    Conversation History:
    
    
    User: Reply with exactly: OK
    
    Provide a brief, helpful response.
    
    ---LLM REPLY---
    OK
    ---Parsed Response (string)---
    "OK"
2026-02-26T02:25:38.392 [BAML INFO] Function QuickResponse:
    Client: FastResponse (us.anthropic.claude-haiku-4-5-20251001-v1:0) - 1839ms. StopReason: end_turn. Tokens(in/out): 41/4
    ---PROMPT---
    user: You are ChoirOS, a helpful AI assistant.
    
    Conversation History:
    
    
    User: Reply with exactly: OK
    
    Provide a brief, helpful response.
    
    ---LLM REPLY---
    OK
    ---Parsed Response (string)---
    "OK"
2026-02-26T02:25:39.544 [BAML INFO] Function QuickResponse:
    Client: FastResponse (glm-4.7) - 1149ms. StopReason: end_turn. Tokens(in/out): 33/2
    ---PROMPT---
    user: You are ChoirOS, a helpful AI assistant.
    
    Conversation History:
    
    
    User: Reply with exactly: OK
    
    Provide a brief, helpful response.
    
    ---LLM REPLY---
    OK
    ---Parsed Response (string)---
    "OK"
2026-02-26T02:25:40.740 [BAML INFO] Function QuickResponse:
    Client: FastResponse (glm-4.7-flash) - 1191ms. StopReason: end_turn. Tokens(in/out): 30/2
    ---PROMPT---
    user: You are ChoirOS, a helpful AI assistant.
    
    Conversation History:
    
    
    User: Reply with exactly: OK
    
    Provide a brief, helpful response.
    
    ---LLM REPLY---
    OK
    ---Parsed Response (string)---
    "OK"
PASS ClaudeBedrockSonnet46 => OK
PASS ClaudeBedrockOpus46 => OK
PASS ClaudeBedrockSonnet45 => OK
PASS ClaudeBedrockHaiku45 => OK
PASS ZaiGLM47 => OK
PASS ZaiGLM47Flash => OK
2026-02-26T02:25:41.620 [BAML INFO] Function QuickResponse:
    Client: FastResponse (kimi-for-coding) - 875ms. StopReason: end_turn. Tokens(in/out): 0/4
    ---PROMPT---
    user: You are ChoirOS, a helpful AI assistant.
    
    Conversation History:
    
    
    User: Reply with exactly: OK
    
    Provide a brief, helpful response.
    
    ---LLM REPLY---
    OK
    ---Parsed Response (string)---
    "OK"
PASS KimiK25 => OK
2026-02-26T02:25:44.035 [BAML INFO] Function QuickResponse:
    Client: FastResponse (glm-5) - 6383ms. StopReason: end_turn. Tokens(in/out): 33/2
    ---PROMPT---
    user: You are ChoirOS, a helpful AI assistant.
    
    Conversation History:
    
    
    User: Reply with exactly: OK
    
    Provide a brief, helpful response.
    
    ---LLM REPLY---
    OK
    ---Parsed Response (string)---
    "OK"
PASS ZaiGLM5 => OK
attempted=8 passed=8
requested_models=ClaudeBedrockOpus46,ClaudeBedrockSonnet46,ClaudeBedrockSonnet45,ClaudeBedrockHaiku45,ZaiGLM5,ZaiGLM47,ZaiGLM47Flash,KimiK25
sampled_models=ClaudeBedrockOpus46,ClaudeBedrockSonnet46,ClaudeBedrockSonnet45,ClaudeBedrockHaiku45,ZaiGLM5,ZaiGLM47,ZaiGLM47Flash,KimiK25
test live_provider_smoke_matrix ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 11.71s


running 1 test
2026-02-26T02:25:49.151 [BAML INFO] Function Decide:
    Client: Orchestrator (us.anthropic.claude-sonnet-4-6) - 2695ms. StopReason: end_turn. Tokens(in/out): 883/116
    ---PROMPT---
    user: You are ChoirOS, an intelligent AI assistant.
    
    You are a planner. Return a valid plan.
    
    Available Tools:
    
    [
      {
        "name": "bash",
        "description": "Execute shell commands",
        "parameters": {"cmd": "string"}
      }
    ]
    
    
    Conversation:
    [{
        "role": "user",
        "content": "Use the bash tool with cmd `printf PLAN_OK` and then summarize.",
    }]
    
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
    
    AgentDecision {
      tool_calls: [
        BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall
      ],
      message: string,
    }
    
    FinishedToolCall {
      tool_name: "finished",
      tool_args: FinishedToolArgs,
      reasoning: string | null,
    }
    
    FinishedToolArgs {
      summary: string | null,
    }
    
    MessageWriterToolCall {
      tool_name: "message_writer",
      tool_args: MessageWriterToolArgs,
      reasoning: string | null,
    }
    
    MessageWriterToolArgs {
      content: string,
      mode: string,
      path: string | null,
      mode_arg: string | null,
    }
    
    FileEditToolCall {
      tool_name: "file_edit",
      tool_args: FileEditToolArgs,
      reasoning: string | null,
    }
    
    FileEditToolArgs {
      path: string,
      old_text: string,
      new_text: string,
    }
    
    FileWriteToolCall {
      tool_name: "file_write",
      tool_args: FileWriteToolArgs,
      reasoning: string | null,
    }
    
    FileWriteToolArgs {
      path: string,
      content: string,
    }
    
    FileReadToolCall {
      tool_name: "file_read",
      tool_args: FileReadToolArgs,
      reasoning: string | null,
    }
    
    FileReadToolArgs {
      path: string,
    }
    
    FetchUrlToolCall {
      tool_name: "fetch_url",
      tool_args: FetchUrlToolArgs,
      reasoning: string | null,
    }
    
    FetchUrlToolArgs {
      path: string,
    }
    
    WebSearchToolCall {
      tool_name: "web_search",
      tool_args: WebSearchToolArgs,
      reasoning: string | null,
    }
    
    WebSearchToolArgs {
      query: string,
    }
    
    BashToolCall {
      tool_name: "bash",
      tool_args: BashToolArgs,
      reasoning: string | null,
    }
    
    BashToolArgs {
      command: string,
    }
    
    Return only valid JSON.
    AgentDecision
    
    ---LLM REPLY---
    ```json
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "The user asked to run the bash command `printf PLAN_OK` and then summarize the result."
        }
      ],
      "message": "Running `printf PLAN_OK` via bash to obtain the output, then I will summarize."
    }
    ```
    ---Parsed Response (class AgentDecision)---
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "The user asked to run the bash command `printf PLAN_OK` and then summarize the result."
        }
      ],
      "message": "Running `printf PLAN_OK` via bash to obtain the output, then I will summarize."
    }
2026-02-26T02:25:50.169 [BAML INFO] Function Decide:
    Client: Orchestrator (us.anthropic.claude-opus-4-6-v1) - 3714ms. StopReason: end_turn. Tokens(in/out): 883/124
    ---PROMPT---
    user: You are ChoirOS, an intelligent AI assistant.
    
    You are a planner. Return a valid plan.
    
    Available Tools:
    
    [
      {
        "name": "bash",
        "description": "Execute shell commands",
        "parameters": {"cmd": "string"}
      }
    ]
    
    
    Conversation:
    [{
        "role": "user",
        "content": "Use the bash tool with cmd `printf PLAN_OK` and then summarize.",
    }]
    
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
    
    AgentDecision {
      tool_calls: [
        BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall
      ],
      message: string,
    }
    
    FinishedToolCall {
      tool_name: "finished",
      tool_args: FinishedToolArgs,
      reasoning: string | null,
    }
    
    FinishedToolArgs {
      summary: string | null,
    }
    
    MessageWriterToolCall {
      tool_name: "message_writer",
      tool_args: MessageWriterToolArgs,
      reasoning: string | null,
    }
    
    MessageWriterToolArgs {
      content: string,
      mode: string,
      path: string | null,
      mode_arg: string | null,
    }
    
    FileEditToolCall {
      tool_name: "file_edit",
      tool_args: FileEditToolArgs,
      reasoning: string | null,
    }
    
    FileEditToolArgs {
      path: string,
      old_text: string,
      new_text: string,
    }
    
    FileWriteToolCall {
      tool_name: "file_write",
      tool_args: FileWriteToolArgs,
      reasoning: string | null,
    }
    
    FileWriteToolArgs {
      path: string,
      content: string,
    }
    
    FileReadToolCall {
      tool_name: "file_read",
      tool_args: FileReadToolArgs,
      reasoning: string | null,
    }
    
    FileReadToolArgs {
      path: string,
    }
    
    FetchUrlToolCall {
      tool_name: "fetch_url",
      tool_args: FetchUrlToolArgs,
      reasoning: string | null,
    }
    
    FetchUrlToolArgs {
      path: string,
    }
    
    WebSearchToolCall {
      tool_name: "web_search",
      tool_args: WebSearchToolArgs,
      reasoning: string | null,
    }
    
    WebSearchToolArgs {
      query: string,
    }
    
    BashToolCall {
      tool_name: "bash",
      tool_args: BashToolArgs,
      reasoning: string | null,
    }
    
    BashToolArgs {
      command: string,
    }
    
    Return only valid JSON.
    AgentDecision
    
    ---LLM REPLY---
    ```json
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "The user requested running `printf PLAN_OK` via the bash tool. Executing this command first, then will summarize the result."
        }
      ],
      "message": "Executing the bash command `printf PLAN_OK` as requested. Will summarize the output afterward."
    }
    ```
    ---Parsed Response (class AgentDecision)---
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "The user requested running `printf PLAN_OK` via the bash tool. Executing this command first, then will summarize the result."
        }
      ],
      "message": "Executing the bash command `printf PLAN_OK` as requested. Will summarize the output afterward."
    }
2026-02-26T02:25:51.912 [BAML INFO] Function Decide:
    Client: Orchestrator (us.anthropic.claude-sonnet-4-5-20250929-v1:0) - 2754ms. StopReason: end_turn. Tokens(in/out): 883/100
    ---PROMPT---
    user: You are ChoirOS, an intelligent AI assistant.
    
    You are a planner. Return a valid plan.
    
    Available Tools:
    
    [
      {
        "name": "bash",
        "description": "Execute shell commands",
        "parameters": {"cmd": "string"}
      }
    ]
    
    
    Conversation:
    [{
        "role": "user",
        "content": "Use the bash tool with cmd `printf PLAN_OK` and then summarize.",
    }]
    
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
    
    AgentDecision {
      tool_calls: [
        BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall
      ],
      message: string,
    }
    
    FinishedToolCall {
      tool_name: "finished",
      tool_args: FinishedToolArgs,
      reasoning: string | null,
    }
    
    FinishedToolArgs {
      summary: string | null,
    }
    
    MessageWriterToolCall {
      tool_name: "message_writer",
      tool_args: MessageWriterToolArgs,
      reasoning: string | null,
    }
    
    MessageWriterToolArgs {
      content: string,
      mode: string,
      path: string | null,
      mode_arg: string | null,
    }
    
    FileEditToolCall {
      tool_name: "file_edit",
      tool_args: FileEditToolArgs,
      reasoning: string | null,
    }
    
    FileEditToolArgs {
      path: string,
      old_text: string,
      new_text: string,
    }
    
    FileWriteToolCall {
      tool_name: "file_write",
      tool_args: FileWriteToolArgs,
      reasoning: string | null,
    }
    
    FileWriteToolArgs {
      path: string,
      content: string,
    }
    
    FileReadToolCall {
      tool_name: "file_read",
      tool_args: FileReadToolArgs,
      reasoning: string | null,
    }
    
    FileReadToolArgs {
      path: string,
    }
    
    FetchUrlToolCall {
      tool_name: "fetch_url",
      tool_args: FetchUrlToolArgs,
      reasoning: string | null,
    }
    
    FetchUrlToolArgs {
      path: string,
    }
    
    WebSearchToolCall {
      tool_name: "web_search",
      tool_args: WebSearchToolArgs,
      reasoning: string | null,
    }
    
    WebSearchToolArgs {
      query: string,
    }
    
    BashToolCall {
      tool_name: "bash",
      tool_args: BashToolArgs,
      reasoning: string | null,
    }
    
    BashToolArgs {
      command: string,
    }
    
    Return only valid JSON.
    AgentDecision
    
    ---LLM REPLY---
    ```json
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "Executing the requested bash command to print PLAN_OK"
        }
      ],
      "message": "Executing bash command `printf PLAN_OK` as requested"
    }
    ```
    ---Parsed Response (class AgentDecision)---
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "Executing the requested bash command to print PLAN_OK"
        }
      ],
      "message": "Executing bash command `printf PLAN_OK` as requested"
    }
2026-02-26T02:25:51.964 [BAML INFO] Function Decide:
    Client: Orchestrator (us.anthropic.claude-haiku-4-5-20251001-v1:0) - 1790ms. StopReason: end_turn. Tokens(in/out): 883/103
    ---PROMPT---
    user: You are ChoirOS, an intelligent AI assistant.
    
    You are a planner. Return a valid plan.
    
    Available Tools:
    
    [
      {
        "name": "bash",
        "description": "Execute shell commands",
        "parameters": {"cmd": "string"}
      }
    ]
    
    
    Conversation:
    [{
        "role": "user",
        "content": "Use the bash tool with cmd `printf PLAN_OK` and then summarize.",
    }]
    
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
    
    AgentDecision {
      tool_calls: [
        BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall
      ],
      message: string,
    }
    
    FinishedToolCall {
      tool_name: "finished",
      tool_args: FinishedToolArgs,
      reasoning: string | null,
    }
    
    FinishedToolArgs {
      summary: string | null,
    }
    
    MessageWriterToolCall {
      tool_name: "message_writer",
      tool_args: MessageWriterToolArgs,
      reasoning: string | null,
    }
    
    MessageWriterToolArgs {
      content: string,
      mode: string,
      path: string | null,
      mode_arg: string | null,
    }
    
    FileEditToolCall {
      tool_name: "file_edit",
      tool_args: FileEditToolArgs,
      reasoning: string | null,
    }
    
    FileEditToolArgs {
      path: string,
      old_text: string,
      new_text: string,
    }
    
    FileWriteToolCall {
      tool_name: "file_write",
      tool_args: FileWriteToolArgs,
      reasoning: string | null,
    }
    
    FileWriteToolArgs {
      path: string,
      content: string,
    }
    
    FileReadToolCall {
      tool_name: "file_read",
      tool_args: FileReadToolArgs,
      reasoning: string | null,
    }
    
    FileReadToolArgs {
      path: string,
    }
    
    FetchUrlToolCall {
      tool_name: "fetch_url",
      tool_args: FetchUrlToolArgs,
      reasoning: string | null,
    }
    
    FetchUrlToolArgs {
      path: string,
    }
    
    WebSearchToolCall {
      tool_name: "web_search",
      tool_args: WebSearchToolArgs,
      reasoning: string | null,
    }
    
    WebSearchToolArgs {
      query: string,
    }
    
    BashToolCall {
      tool_name: "bash",
      tool_args: BashToolArgs,
      reasoning: string | null,
    }
    
    BashToolArgs {
      command: string,
    }
    
    Return only valid JSON.
    AgentDecision
    
    ---LLM REPLY---
    ```json
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "Execute the requested bash command to print PLAN_OK"
        }
      ],
      "message": "Executing bash command to print PLAN_OK and then will summarize the result."
    }
    ```
    ---Parsed Response (class AgentDecision)---
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "Execute the requested bash command to print PLAN_OK"
        }
      ],
      "message": "Executing bash command to print PLAN_OK and then will summarize the result."
    }
2026-02-26T02:25:53.712 [BAML INFO] Function Decide:
    Client: Orchestrator (glm-4.7) - 1743ms. StopReason: end_turn. Tokens(in/out): 714/79
    ---PROMPT---
    user: You are ChoirOS, an intelligent AI assistant.
    
    You are a planner. Return a valid plan.
    
    Available Tools:
    
    [
      {
        "name": "bash",
        "description": "Execute shell commands",
        "parameters": {"cmd": "string"}
      }
    ]
    
    
    Conversation:
    [{
        "role": "user",
        "content": "Use the bash tool with cmd `printf PLAN_OK` and then summarize.",
    }]
    
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
    
    AgentDecision {
      tool_calls: [
        BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall
      ],
      message: string,
    }
    
    FinishedToolCall {
      tool_name: "finished",
      tool_args: FinishedToolArgs,
      reasoning: string | null,
    }
    
    FinishedToolArgs {
      summary: string | null,
    }
    
    MessageWriterToolCall {
      tool_name: "message_writer",
      tool_args: MessageWriterToolArgs,
      reasoning: string | null,
    }
    
    MessageWriterToolArgs {
      content: string,
      mode: string,
      path: string | null,
      mode_arg: string | null,
    }
    
    FileEditToolCall {
      tool_name: "file_edit",
      tool_args: FileEditToolArgs,
      reasoning: string | null,
    }
    
    FileEditToolArgs {
      path: string,
      old_text: string,
      new_text: string,
    }
    
    FileWriteToolCall {
      tool_name: "file_write",
      tool_args: FileWriteToolArgs,
      reasoning: string | null,
    }
    
    FileWriteToolArgs {
      path: string,
      content: string,
    }
    
    FileReadToolCall {
      tool_name: "file_read",
      tool_args: FileReadToolArgs,
      reasoning: string | null,
    }
    
    FileReadToolArgs {
      path: string,
    }
    
    FetchUrlToolCall {
      tool_name: "fetch_url",
      tool_args: FetchUrlToolArgs,
      reasoning: string | null,
    }
    
    FetchUrlToolArgs {
      path: string,
    }
    
    WebSearchToolCall {
      tool_name: "web_search",
      tool_args: WebSearchToolArgs,
      reasoning: string | null,
    }
    
    WebSearchToolArgs {
      query: string,
    }
    
    BashToolCall {
      tool_name: "bash",
      tool_args: BashToolArgs,
      reasoning: string | null,
    }
    
    BashToolArgs {
      command: string,
    }
    
    Return only valid JSON.
    AgentDecision
    
    ---LLM REPLY---
    ```json
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "User requested to execute the bash command 'printf PLAN_OK'."
        }
      ],
      "message": "Executing the bash command to print 'PLAN_OK'."
    }
    ```
    ---Parsed Response (class AgentDecision)---
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "User requested to execute the bash command 'printf PLAN_OK'."
        }
      ],
      "message": "Executing the bash command to print 'PLAN_OK'."
    }
2026-02-26T02:25:55.946 [BAML INFO] Function Decide:
    Client: Orchestrator (glm-4.7-flash) - 2228ms. StopReason: end_turn. Tokens(in/out): 706/78
    ---PROMPT---
    user: You are ChoirOS, an intelligent AI assistant.
    
    You are a planner. Return a valid plan.
    
    Available Tools:
    
    [
      {
        "name": "bash",
        "description": "Execute shell commands",
        "parameters": {"cmd": "string"}
      }
    ]
    
    
    Conversation:
    [{
        "role": "user",
        "content": "Use the bash tool with cmd `printf PLAN_OK` and then summarize.",
    }]
    
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
    
    AgentDecision {
      tool_calls: [
        BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall
      ],
      message: string,
    }
    
    FinishedToolCall {
      tool_name: "finished",
      tool_args: FinishedToolArgs,
      reasoning: string | null,
    }
    
    FinishedToolArgs {
      summary: string | null,
    }
    
    MessageWriterToolCall {
      tool_name: "message_writer",
      tool_args: MessageWriterToolArgs,
      reasoning: string | null,
    }
    
    MessageWriterToolArgs {
      content: string,
      mode: string,
      path: string | null,
      mode_arg: string | null,
    }
    
    FileEditToolCall {
      tool_name: "file_edit",
      tool_args: FileEditToolArgs,
      reasoning: string | null,
    }
    
    FileEditToolArgs {
      path: string,
      old_text: string,
      new_text: string,
    }
    
    FileWriteToolCall {
      tool_name: "file_write",
      tool_args: FileWriteToolArgs,
      reasoning: string | null,
    }
    
    FileWriteToolArgs {
      path: string,
      content: string,
    }
    
    FileReadToolCall {
      tool_name: "file_read",
      tool_args: FileReadToolArgs,
      reasoning: string | null,
    }
    
    FileReadToolArgs {
      path: string,
    }
    
    FetchUrlToolCall {
      tool_name: "fetch_url",
      tool_args: FetchUrlToolArgs,
      reasoning: string | null,
    }
    
    FetchUrlToolArgs {
      path: string,
    }
    
    WebSearchToolCall {
      tool_name: "web_search",
      tool_args: WebSearchToolArgs,
      reasoning: string | null,
    }
    
    WebSearchToolArgs {
      query: string,
    }
    
    BashToolCall {
      tool_name: "bash",
      tool_args: BashToolArgs,
      reasoning: string | null,
    }
    
    BashToolArgs {
      command: string,
    }
    
    Return only valid JSON.
    AgentDecision
    
    ---LLM REPLY---
    ```json
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf 'PLAN_OK'"
          },
          "reasoning": "Execute the user's requested command to produce the PLAN_OK output."
        }
      ],
      "message": "Executing the requested command to output PLAN_OK."
    }
    ```
    ---Parsed Response (class AgentDecision)---
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf 'PLAN_OK'"
          },
          "reasoning": "Execute the user's requested command to produce the PLAN_OK output."
        }
      ],
      "message": "Executing the requested command to output PLAN_OK."
    }
PASS ClaudeBedrockSonnet46 => action=tool_call tool_calls=1
PASS ClaudeBedrockOpus46 => action=tool_call tool_calls=1
PASS ClaudeBedrockSonnet45 => action=tool_call tool_calls=1
PASS ClaudeBedrockHaiku45 => action=tool_call tool_calls=1
PASS ZaiGLM47 => action=tool_call tool_calls=1
PASS ZaiGLM47Flash => action=tool_call tool_calls=1
2026-02-26T02:25:56.114 [BAML INFO] Function Decide:
    Client: Orchestrator (glm-5) - 4196ms. StopReason: end_turn. Tokens(in/out): 714/74
    ---PROMPT---
    user: You are ChoirOS, an intelligent AI assistant.
    
    You are a planner. Return a valid plan.
    
    Available Tools:
    
    [
      {
        "name": "bash",
        "description": "Execute shell commands",
        "parameters": {"cmd": "string"}
      }
    ]
    
    
    Conversation:
    [{
        "role": "user",
        "content": "Use the bash tool with cmd `printf PLAN_OK` and then summarize.",
    }]
    
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
    
    AgentDecision {
      tool_calls: [
        BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall
      ],
      message: string,
    }
    
    FinishedToolCall {
      tool_name: "finished",
      tool_args: FinishedToolArgs,
      reasoning: string | null,
    }
    
    FinishedToolArgs {
      summary: string | null,
    }
    
    MessageWriterToolCall {
      tool_name: "message_writer",
      tool_args: MessageWriterToolArgs,
      reasoning: string | null,
    }
    
    MessageWriterToolArgs {
      content: string,
      mode: string,
      path: string | null,
      mode_arg: string | null,
    }
    
    FileEditToolCall {
      tool_name: "file_edit",
      tool_args: FileEditToolArgs,
      reasoning: string | null,
    }
    
    FileEditToolArgs {
      path: string,
      old_text: string,
      new_text: string,
    }
    
    FileWriteToolCall {
      tool_name: "file_write",
      tool_args: FileWriteToolArgs,
      reasoning: string | null,
    }
    
    FileWriteToolArgs {
      path: string,
      content: string,
    }
    
    FileReadToolCall {
      tool_name: "file_read",
      tool_args: FileReadToolArgs,
      reasoning: string | null,
    }
    
    FileReadToolArgs {
      path: string,
    }
    
    FetchUrlToolCall {
      tool_name: "fetch_url",
      tool_args: FetchUrlToolArgs,
      reasoning: string | null,
    }
    
    FetchUrlToolArgs {
      path: string,
    }
    
    WebSearchToolCall {
      tool_name: "web_search",
      tool_args: WebSearchToolArgs,
      reasoning: string | null,
    }
    
    WebSearchToolArgs {
      query: string,
    }
    
    BashToolCall {
      tool_name: "bash",
      tool_args: BashToolArgs,
      reasoning: string | null,
    }
    
    BashToolArgs {
      command: string,
    }
    
    Return only valid JSON.
    AgentDecision
    
    ---LLM REPLY---
    ```json
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "User requested to run this specific bash command"
        }
      ],
      "message": "Executing the bash command as requested by the user"
    }
    ```
    ---Parsed Response (class AgentDecision)---
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "User requested to run this specific bash command"
        }
      ],
      "message": "Executing the bash command as requested by the user"
    }
PASS ZaiGLM5 => action=tool_call tool_calls=1
2026-02-26T02:25:58.873 [BAML INFO] Function Decide:
    Client: Orchestrator (kimi-for-coding) - 2923ms. StopReason: end_turn. Tokens(in/out): 0/78
    ---PROMPT---
    user: You are ChoirOS, an intelligent AI assistant.
    
    You are a planner. Return a valid plan.
    
    Available Tools:
    
    [
      {
        "name": "bash",
        "description": "Execute shell commands",
        "parameters": {"cmd": "string"}
      }
    ]
    
    
    Conversation:
    [{
        "role": "user",
        "content": "Use the bash tool with cmd `printf PLAN_OK` and then summarize.",
    }]
    
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
    
    AgentDecision {
      tool_calls: [
        BashToolCall | WebSearchToolCall | FetchUrlToolCall | FileReadToolCall | FileWriteToolCall | FileEditToolCall | MessageWriterToolCall | FinishedToolCall
      ],
      message: string,
    }
    
    FinishedToolCall {
      tool_name: "finished",
      tool_args: FinishedToolArgs,
      reasoning: string | null,
    }
    
    FinishedToolArgs {
      summary: string | null,
    }
    
    MessageWriterToolCall {
      tool_name: "message_writer",
      tool_args: MessageWriterToolArgs,
      reasoning: string | null,
    }
    
    MessageWriterToolArgs {
      content: string,
      mode: string,
      path: string | null,
      mode_arg: string | null,
    }
    
    FileEditToolCall {
      tool_name: "file_edit",
      tool_args: FileEditToolArgs,
      reasoning: string | null,
    }
    
    FileEditToolArgs {
      path: string,
      old_text: string,
      new_text: string,
    }
    
    FileWriteToolCall {
      tool_name: "file_write",
      tool_args: FileWriteToolArgs,
      reasoning: string | null,
    }
    
    FileWriteToolArgs {
      path: string,
      content: string,
    }
    
    FileReadToolCall {
      tool_name: "file_read",
      tool_args: FileReadToolArgs,
      reasoning: string | null,
    }
    
    FileReadToolArgs {
      path: string,
    }
    
    FetchUrlToolCall {
      tool_name: "fetch_url",
      tool_args: FetchUrlToolArgs,
      reasoning: string | null,
    }
    
    FetchUrlToolArgs {
      path: string,
    }
    
    WebSearchToolCall {
      tool_name: "web_search",
      tool_args: WebSearchToolArgs,
      reasoning: string | null,
    }
    
    WebSearchToolArgs {
      query: string,
    }
    
    BashToolCall {
      tool_name: "bash",
      tool_args: BashToolArgs,
      reasoning: string | null,
    }
    
    BashToolArgs {
      command: string,
    }
    
    Return only valid JSON.
    AgentDecision
    
    ---LLM REPLY---
    ```json
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "Execute the requested bash command to print PLAN_OK"
        }
      ],
      "message": "Executing the bash command to print PLAN_OK as requested."
    }
    ```
    ---Parsed Response (class AgentDecision)---
    {
      "tool_calls": [
        {
          "tool_name": "bash",
          "tool_args": {
            "command": "printf PLAN_OK"
          },
          "reasoning": "Execute the requested bash command to print PLAN_OK"
        }
      ],
      "message": "Executing the bash command to print PLAN_OK as requested."
    }
PASS KimiK25 => action=tool_call tool_calls=1
decide attempted=8 passed=8
decide requested_models=ClaudeBedrockOpus46,ClaudeBedrockSonnet46,ClaudeBedrockSonnet45,ClaudeBedrockHaiku45,ZaiGLM5,ZaiGLM47,ZaiGLM47Flash,KimiK25
decide sampled_models=ClaudeBedrockOpus46,ClaudeBedrockSonnet46,ClaudeBedrockSonnet45,ClaudeBedrockHaiku45,ZaiGLM5,ZaiGLM47,ZaiGLM47Flash,KimiK25
test live_decide_matrix ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 14.59s

Skipping gateway search matrix (--skip-gateway-search).

== Provider Matrix Summary ==
PASS model_provider_live_test/live_provider_smoke_matrix
PASS model_provider_live_test/live_decide_matrix
passes=2 failures=0
```
