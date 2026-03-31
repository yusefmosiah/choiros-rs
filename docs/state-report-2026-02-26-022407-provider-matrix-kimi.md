# Provider Matrix Report (2026-02-26-022407)
Date: 2026-02-26
Kind: Report
Status: Active
Requires: []

## Command
```bash
./scripts/ops/validate-local-provider-matrix.sh --models "KimiK25"
```

## Output
```text
== Model provider matrix ==
models=KimiK25

running 1 test
2026-02-26T02:24:31.114 [BAML INFO] Function QuickResponse:
    Client: FastResponse (kimi-for-coding) - 1095ms. StopReason: end_turn. Tokens(in/out): 35/4
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
attempted=1 passed=1
requested_models=KimiK25
sampled_models=KimiK25
test live_provider_smoke_matrix ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 4.29s


running 1 test
2026-02-26T02:24:36.749 [BAML INFO] Function Decide:
    Client: Orchestrator (kimi-for-coding) - 2938ms. StopReason: end_turn. Tokens(in/out): 715/79
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
      "message": "Executing the bash command `printf PLAN_OK` as requested."
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
      "message": "Executing the bash command `printf PLAN_OK` as requested."
    }
PASS KimiK25 => action=tool_call tool_calls=1
decide attempted=1 passed=1
decide requested_models=KimiK25
decide sampled_models=KimiK25
test live_decide_matrix ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 5.10s


== Provider Matrix Summary ==
PASS model_provider_live_test/live_provider_smoke_matrix
PASS model_provider_live_test/live_decide_matrix
FAIL gateway-search-preflight: missing CHOIR_PROVIDER_GATEWAY_TOKEN / --gateway-token
passes=2 failures=1
```
