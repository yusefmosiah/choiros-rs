# Go Agent Runtime Research Report

> Generated: 2026-04-08 | Context: ChoirOS unified multiagent system design

---

## 1. Agent Frameworks in Go

### Google ADK-Go (Agent Development Kit) — ★ Top Pick

- **Repo**: [google/adk-go](https://github.com/google/adk-go) — `google.golang.org/adk`
- **Status**: **v1.0 GA** (March 2026). Production-grade, actively maintained by Google.
- **Key features**:
  - Code-first, idiomatic Go agent definitions with strong typing
  - Built-in **multi-agent orchestration**: supervisor agents delegate to sub-agents
  - Native **A2A (Agent-to-Agent) protocol** support for cross-agent interop
  - **Tool calling** with typed Go functions registered as tools
  - **OpenTelemetry** tracing built-in for observability
  - Human-in-the-loop security model with approval flows
  - Self-healing plugins for resilient agent execution
  - Supports Gemini, but pluggable model interface
- **Applicability**: Directly models the appagent→worker hierarchy. The supervisor/sub-agent pattern maps cleanly to our "appagent supervises workers" requirement. A2A support enables the "one standardized API for local and remote agents" vision.

### LangChainGo

- **Repo**: [tmc/langchaingo](https://github.com/tmc/langchaingo) — `github.com/tmc/langchaingo`
- **Status**: Active, community-driven. Go port of LangChain.
- **Key features**:
  - Unified LLM interface (`llms` package) with adapters for OpenAI, Anthropic, Google, Ollama, etc.
  - Agents with tool selection (ReAct pattern)
  - Chains, memory, document loaders, vector stores, embeddings
  - Good ecosystem of integrations
- **Applicability**: Useful as a **library layer** for LLM provider abstraction and tool-calling patterns. Less opinionated about agent topology than ADK-Go. Could serve as the LLM-calling substrate underneath a custom runtime.

### Genkit (Firebase/Google)

- **Repo**: [genkit-ai/genkit](https://github.com/firebase/genkit) — `github.com/firebase/genkit/go`
- **Status**: Active, Google-maintained. Multi-language (JS, Go, Python).
- **Key features**:
  - Unified APIs for Gemini, GPT, Claude, and more
  - Structured outputs, tool calling, agentic workflows
  - Built-in developer tools and production monitoring
  - Flow-based composition of AI operations
- **Applicability**: Good for building individual agent logic with structured output. Less focused on multi-agent orchestration than ADK-Go.

### Eino (ByteDance/CloudWeGo)

- **Repo**: [cloudwego/eino](https://www.cloudwego.io/docs/eino/overview/)
- **Status**: Open-sourced by ByteDance, active development.
- **Key features**:
  - Comprehensive LLM application framework in Go
  - ReAct agents, WorkflowAgents, Supervisor pattern, Plan-Execute-Replan
  - Composable, interruptible multi-agent workflows
  - DeepAgents pattern for complex reasoning
- **Applicability**: Strong multi-agent patterns. The Supervisor agent pattern directly maps to our appagent model. Worth evaluating alongside ADK-Go for orchestration.

### OpenAI Agents Go (Community)

- **Repo**: [nlpodyssey/openai-agents-go](https://github.com/nlpodyssey/openai-agents-go)
- **Status**: Community port of OpenAI's Python Agents SDK to Go.
- **Key features**:
  - Lightweight multi-agent workflow framework
  - Agent handoffs, tool calling, guardrails
  - Mirrors OpenAI Agents SDK patterns
- **Applicability**: Good reference implementation. Less mature than ADK-Go but demonstrates the pattern well.

### Other Notable Mentions

| Library | Description | Notes |
|---------|-------------|-------|
| **Anyi** | Lightweight Go AI agent framework | Minimal, good for custom builds |
| **Jetify AI SDK** | Go AI SDK for agent development | Newer entrant |
| **pontus-devoteam/agent-sdk-go** | Multi-LLM agent SDK inspired by OpenAI | Supports function calling, handoffs |

---

## 2. Actor/Messaging Patterns

### Proto.Actor

- **Repo**: [asynkron/protoactor-go](https://github.com/asynkron/protoactor-go)
- **Status**: Mature, actively maintained. Multi-language (Go, C#, Java/Kotlin).
- **Key features**:
  - Ultra-fast distributed actor framework
  - Location transparency — actors communicate via PID regardless of local/remote
  - Cluster support with gossip-based membership
  - Supervision strategies (one-for-one, all-for-one, restarting, stopping)
  - Virtual actors (grains) similar to Microsoft Orleans
  - Proto.Remote for cross-process communication via gRPC
- **Applicability**: **Excellent fit** for the appagent→worker model. Supervision trees directly model our hierarchy. Location transparency solves "same API for local and remote" requirement. Virtual actors could represent agent instances.

### Ergo Framework

- **Repo**: [ergo-services/ergo](https://github.com/ergo-services/ergo)
- **Status**: Active, zero dependencies. Inspired by Erlang/OTP.
- **Key features**:
  - Full actor model with Erlang-style supervision trees
  - Network transparency — actors communicate across nodes seamlessly
  - Process registry, monitors, links
  - Cloud-native observability
  - GenServer, Supervisor, Application patterns from OTP
- **Applicability**: **Strong fit** for supervision and fault tolerance. The Erlang-inspired model is proven for building resilient distributed systems. "Let it crash" philosophy works well for agent workers that may fail.

### Hollywood

- **Repo**: [anthdm/hollywood](https://github.com/anthdm/hollywood)
- **Status**: Active, lightweight.
- **Key features**:
  - Blazingly fast, minimalist actor engine
  - Simple API: `Receive(ctx *actor.Context)` interface
  - Remote actors over TCP
  - Cluster support
- **Applicability**: Good if we want a lightweight actor substrate. Less batteries-included than Proto.Actor or Ergo.

### Pattern: Appagent-Supervises-Workers

The actor model naturally fits our architecture:

```
AppAgent (Supervisor Actor)
├── WorkerAgent-1 (Child Actor) — local execution
├── WorkerAgent-2 (Child Actor) — local execution
└── WorkerAgent-3 (Remote Actor) — remote VM execution
    └── (same message interface, location transparent)
```

- **Supervision**: AppAgent defines restart strategies for failed workers
- **Messaging**: Type-safe message passing between agents via actor PIDs
- **Location transparency**: Workers can be local goroutines or remote processes — same `Send(pid, msg)` API
- **Lease/capability**: Actor PIDs can serve as capability tokens; only holders can send messages

---

## 3. MCP and A2A in Go

### MCP (Model Context Protocol) — Go SDKs

#### Official Go SDK (Google-maintained)

- **Repo**: [modelcontextprotocol/go-sdk](https://github.com/modelcontextprotocol/go-sdk) — `github.com/modelcontextprotocol/go-sdk`
- **Status**: Official SDK, maintained in collaboration with Google.
- **Key features**:
  - Full MCP client and server implementation
  - Tool registration and invocation
  - Resource exposure (files, data sources)
  - Prompt templates
  - Stdio and HTTP/SSE transport
- **Also notable**: `golang.org/x/tools/internal/mcp` — Go team's own internal MCP implementation in the tools repo

#### mcp-go (Community)

- **Repo**: [mark3labs/mcp-go](https://github.com/mark3labs/mcp-go)
- **Status**: Popular community implementation, well-tested.
- **Key features**:
  - Clean Go API for MCP servers and clients
  - Tool, resource, and prompt handlers
  - Multiple transport options

#### Relation to Our Requirements

MCP standardizes how agents access **tools and resources**. For our system:
- Each agent's file read/write, tool calling capabilities → exposed as MCP tools
- Agent sandboxes (microVMs) can run MCP servers exposing their capabilities
- The appagent can be an MCP client consuming worker capabilities
- **MCP is the tool/resource layer** — it answers "how does an agent call tools?"

### A2A (Agent-to-Agent Protocol)

- **Spec**: [a2a-protocol.org](https://a2a-protocol.org/latest/specification/)
- **Repo**: [a2aproject/A2A](https://github.com/a2aproject/A2A)
- **Status**: Linux Foundation project. GA specification. Go samples in the repo.
- **Key concepts**:
  - **Agent Card**: JSON metadata describing an agent's capabilities, skills, endpoint
  - **Task**: Unit of work sent between agents (lifecycle: submitted → working → completed/failed)
  - **Message/Part**: Structured communication within tasks (text, file, data parts)
  - **Streaming**: SSE-based streaming for long-running tasks
  - **Push Notifications**: Webhook-based async updates
- **Transport**: HTTP + JSON-RPC 2.0, with SSE for streaming
- **Key fit**: A2A is the **agent-to-agent communication layer**. It answers "how do agents talk to each other?" This is complementary to MCP (which handles agent-to-tool communication).

#### ADK-Go + A2A Integration

ADK-Go has **native A2A support**, meaning:
- ADK agents can be exposed as A2A-compatible services
- Any A2A-compliant agent (regardless of framework) can communicate with ADK agents
- This gives us the "one standardized API" for both local and remote agents

### MCP + A2A Together = Our Standardized API

```
┌─────────────────────────────────────┐
│         A2A Protocol Layer          │  Agent ↔ Agent communication
│   (task delegation, status, streaming)│
├─────────────────────────────────────┤
│         MCP Protocol Layer          │  Agent ↔ Tool/Resource access
│   (file I/O, tool calling, prompts) │
├─────────────────────────────────────┤
│       Agent Runtime (Go)            │  Execution engine
│   (ADK-Go / custom runtime)        │
└─────────────────────────────────────┘
```

---

## 4. Web Desktop Patterns

### htmx + templ — Server-Side Rendered Reactive UI

#### templ

- **Repo**: [a-h/templ](https://github.com/a-h/templ)
- **Status**: Mature, widely adopted in Go community. Type-safe HTML templating.
- **Key features**:
  - Compile-time type-checked Go templates
  - Generates pure Go code — no runtime template parsing
  - Component-based architecture (composable templ components)
  - IDE support with LSP
  - Works with any Go HTTP framework

#### htmx

- **Status**: v2.x, extremely popular. "HTML over the wire" philosophy.
- **Key features**:
  - Any HTML element can issue HTTP requests (GET, POST, PUT, DELETE)
  - Swap response HTML into DOM targets
  - WebSocket and SSE extensions built-in
  - CSS transitions, request indicators
  - No JavaScript build step required
- **Desktop-like UX patterns**:
  - `hx-trigger="every 1s"` for polling
  - `hx-ext="ws"` for WebSocket connections
  - `hx-ext="sse"` for server-sent events
  - Out-of-band swaps (`hx-swap-oob`) for multi-region updates
  - View transitions API integration for smooth page transitions

#### Go + templ + htmx Stack

This is the **dominant Go web UI stack in 2026**. Multiple scaffolding tools exist. The pattern:

```go
// templ component
templ AgentStatus(agent Agent) {
    <div id="agent-status" hx-get="/agents/{agent.ID}/status" hx-trigger="every 2s">
        <span class={ statusClass(agent.Status) }>{ agent.Status }</span>
    </div>
}
```

### WebSocket Libraries

| Library | Notes |
|---------|-------|
| **gorilla/websocket** | De facto standard. Battle-tested. Widely used. |
| **coder/websocket** (nhooyr) | Modern, idiomatic. Uses `context.Context`. Minimal API. Recommended for new projects. |

**coder/websocket** is the recommended choice for new projects:
- Proper `context.Context` integration
- Graceful shutdown support
- Concurrent-safe by default
- Smaller API surface

### Server-Sent Events (SSE) Libraries

| Library | Notes |
|---------|-------|
| **tmaxmax/go-sse** | Fully spec-compliant. Feature-rich. |
| **r3labs/sse** | Server and client. Simple API. |
| **go.jetify.com/sse** | Tiny, zero-dependency. Recent (2025). |

SSE is ideal for:
- Agent status streaming (task progress, logs)
- A2A protocol streaming responses
- Real-time UI updates without full WebSocket overhead

### Recommended Web Architecture

```
Browser (htmx + Alpine.js for client state)
    │
    ├── HTTP/HTML ──→ Go server (templ components)
    ├── SSE ────────→ Agent status streams
    └── WebSocket ──→ Interactive agent sessions (terminal, chat)
```

**Alpine.js** (lightweight JS framework) pairs well with htmx for client-side state management that htmx doesn't handle (modals, dropdowns, local form state).

---

## 5. LLM Provider Abstraction

### any-llm-go (Mozilla.ai) — ★ Purpose-Built

- **Repo**: [mozilla-ai/any-llm-go](https://github.com/mozilla-ai/any-llm-go) — `github.com/mozilla-ai/any-llm-go`
- **Status**: Released March 2026 by Mozilla.ai. Active development.
- **Key features**:
  - **One interface, many providers**: OpenAI, Anthropic Claude, Mistral, Google Gemini, Llamafile, Ollama
  - Type-safe provider configuration
  - Channel-based streaming (idiomatic Go)
  - Normalized errors across providers
  - Simple API: `client.Chat(ctx, messages, opts...)`
- **Applicability**: Purpose-built for exactly our "provider-agnostic adapter" requirement. Lightweight, focused.

### go-llm (mutablelogic)

- **Repo**: [mutablelogic/go-llm](https://github.com/mutablelogic/go-llm)
- **Status**: Active, feature-rich.
- **Key features**:
  - Multi-provider: OpenAI, Anthropic, Google Gemini, Mistral, Ollama
  - **Structured output** via JSON schema
  - **Tool/function calling** support
  - **Agent framework** built on top (tool-using agents)
  - Extended thinking/reasoning support (Anthropic, Gemini)
  - OpenTelemetry tracing
- **Applicability**: More batteries-included than any-llm-go. The structured output and tool calling support are directly useful.

### LangChainGo LLMs Package

- **Package**: `github.com/tmc/langchaingo/llms`
- **Providers**: OpenAI, Anthropic, Google, Ollama, Cohere, HuggingFace, and many more
- **Features**: Unified `llms.Model` interface, streaming, function calling
- **Applicability**: Broadest provider support. Heavier dependency (pulls in all of langchaingo).

### Genkit Go

- **Package**: `github.com/firebase/genkit/go`
- **Features**: Unified text generation, structured output, tool calling across providers
- **Applicability**: Google-maintained, production-grade. Good if already using ADK-Go.

### Comparison Matrix

| Library | Providers | Tool Calling | Structured Output | Streaming | Weight |
|---------|-----------|-------------|-------------------|-----------|--------|
| any-llm-go | 6+ | ✗ | ✗ | ✓ (channels) | Minimal |
| go-llm | 5+ | ✓ | ✓ (JSON schema) | ✓ | Medium |
| langchaingo/llms | 10+ | ✓ | ✓ | ✓ | Heavy |
| Genkit Go | 4+ | ✓ | ✓ | ✓ | Medium |
| ADK-Go (built-in) | Gemini + pluggable | ✓ | ✓ | ✓ | Part of ADK |

### Recommendation

Use **go-llm** or **Genkit Go** as the provider abstraction layer — they provide tool calling and structured output which are essential for agent tool use. If using ADK-Go as the orchestration framework, its built-in model interface may suffice, with custom adapters for non-Gemini providers.

---

## 6. MicroVM Integration

### firecracker-go-sdk — ★ Primary Choice for Firecracker

- **Repo**: [firecracker-microvm/firecracker-go-sdk](https://github.com/firecracker-microvm/firecracker-go-sdk)
- **Status**: Official AWS SDK. Maintained by the Firecracker team.
- **Key features**:
  - Full Firecracker API coverage via Go types
  - VM lifecycle: create, start, stop, pause, resume
  - Drive management (rootfs, additional drives)
  - Network interface configuration (tap devices)
  - Vsock support for host↔guest communication
  - Snapshot/restore for fast cold starts
  - Rate limiters for I/O and network
  - Metrics and logging configuration
- **Architecture**:
  ```go
  cfg := firecracker.Config{
      SocketPath:      "/tmp/firecracker.sock",
      KernelImagePath: "vmlinux",
      Drives:          drives,
      MachineCfg:      machineConfig,
  }
  m, _ := firecracker.NewMachine(ctx, firecracker.WithConfig(cfg))
  m.Start(ctx)
  ```
- **Production usage**: Powers AWS Lambda, Fly.io, and many other serverless platforms.

### cloud-hypervisor-go

- **Repo**: [afritzler/cloud-hypervisor-go](https://github.com/afritzler/cloud-hypervisor-go)
- **Status**: Community SDK for Cloud Hypervisor's REST API.
- **Features**: VM create, boot, shutdown, info, resize via HTTP API
- **Applicability**: If using cloud-hypervisor instead of Firecracker. Less mature than firecracker-go-sdk.

### Flintlock (Weaveworks)

- **Repo**: Formerly weaveworks-liquidmetal/flintlock
- **Features**: Higher-level microVM management, supports both Firecracker and Cloud Hypervisor
- **Status**: Weaveworks shut down; community maintenance uncertain.

### VM Lifecycle Management Patterns

For our sandboxed agent execution:

```
Agent Worker Request
    │
    ▼
VM Pool Manager (Go)
    │
    ├── Pre-warmed VM pool (snapshot/restore for fast starts)
    ├── Vsock communication channel (host ↔ guest)
    ├── Drive mounting (agent workspace filesystem)
    └── Resource limits (CPU, memory, I/O rate limiting)
    │
    ▼
Firecracker MicroVM
    │
    ├── Agent runtime process (inside VM)
    ├── MCP server (exposing tools to host)
    └── Vsock client (communication back to host)
```

**Key patterns**:
- **VM pooling**: Pre-boot VMs from snapshots for sub-100ms cold starts
- **Vsock**: Use vsock (not networking) for host↔guest IPC — faster and no network config needed
- **Overlay filesystems**: Use overlayfs for copy-on-write agent workspaces
- **Resource quotas**: Firecracker rate limiters for CPU/IO per agent

### Go Process Supervision (Non-VM)

For local (non-sandboxed) agent workers:

- **Actor supervision** (Proto.Actor/Ergo): Built-in restart strategies
- **`os/exec` + context**: Standard Go process management with timeout
- **hashicorp/go-plugin**: gRPC-based plugin system with process isolation (used by Terraform, Vault)
  - Agents as plugins with defined interfaces
  - Automatic process restart on crash
  - Version negotiation

---

## 7. Recommended Stack

### Core Runtime

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| **Agent Orchestration** | **Google ADK-Go** | Production-grade, native A2A + multi-agent support, supervisor pattern |
| **Actor Substrate** | **Proto.Actor** or **Ergo** | Supervision trees, location transparency, fault tolerance |
| **Agent Protocol** | **A2A** (agent↔agent) + **MCP** (agent↔tools) | Open standards, ADK-Go native support |
| **LLM Abstraction** | **go-llm** or **Genkit Go** | Multi-provider, tool calling, structured output |
| **MCP SDK** | **modelcontextprotocol/go-sdk** | Official, Google co-maintained |

### Web Surface

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| **Templating** | **templ** | Type-safe, compiled, component-based |
| **Interactivity** | **htmx** | Server-driven UI, no JS build step |
| **Client state** | **Alpine.js** | Lightweight client-side reactivity |
| **WebSocket** | **coder/websocket** | Modern, context-aware, concurrent-safe |
| **SSE** | **tmaxmax/go-sse** | Spec-compliant, for streaming agent status |
| **HTTP router** | **net/http** (Go 1.22+) or **chi** | Standard library is now sufficient |

### Infrastructure

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| **MicroVM (Firecracker)** | **firecracker-go-sdk** | Official SDK, full API coverage |
| **MicroVM (cloud-hypervisor)** | **cloud-hypervisor-go** | REST API wrapper |
| **Host↔Guest IPC** | **Vsock** | Low-latency, no network stack needed |
| **Process plugins** | **hashicorp/go-plugin** | For local agent process isolation |
| **Observability** | **OpenTelemetry Go SDK** | ADK-Go native integration |

---

## 8. Architecture Sketch

```
┌──────────────────────────────────────────────────────────────────────┐
│                        WEB DESKTOP (Browser)                        │
│                                                                     │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────────────────┐  │
│  │   htmx      │  │  Alpine.js   │  │  WebSocket (agent chat)   │  │
│  │  (requests)  │  │ (local state)│  │  SSE (status streams)     │  │
│  └──────┬──────┘  └──────┬───────┘  └────────────┬──────────────┘  │
└─────────┼────────────────┼───────────────────────┼──────────────────┘
          │                │                       │
          ▼                ▼                       ▼
┌──────────────────────────────────────────────────────────────────────┐
│                      GO HTTP SERVER                                  │
│                                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────────────┐  │
│  │  templ views  │  │  REST API    │  │  WS/SSE handlers          │  │
│  │  (HTML render)│  │  (JSON)      │  │  (coder/websocket, go-sse)│  │
│  └──────┬───────┘  └──────┬───────┘  └────────────┬──────────────┘  │
└─────────┼────────────────┼───────────────────────┼──────────────────┘
          │                │                       │
          ▼                ▼                       ▼
┌──────────────────────────────────────────────────────────────────────┐
│                     APP STATE & SESSION LAYER                        │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  User + AppAgent = peer canonical editors of app state         │  │
│  │  Workers = subordinate non-canonical executors                 │  │
│  │  State: SQLite/Postgres + event log                            │  │
│  └────────────────────────────────────────────────────────────────┘  │
└───────────────────────────┬──────────────────────────────────────────┘
                            │
                            ▼
┌──────────────────────────────────────────────────────────────────────┐
│                    AGENT ORCHESTRATION LAYER                         │
│                      (ADK-Go + Actor System)                        │
│                                                                     │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │                    AppAgent (Supervisor)                        │  │
│  │                                                                │  │
│  │  • Canonical editor of app state                               │  │
│  │  • Delegates tasks to workers via A2A                          │  │
│  │  • Monitors worker health (actor supervision)                  │  │
│  │  • Aggregates results, updates state                           │  │
│  │                                                                │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐  │  │
│  │  │ Worker 1 │  │ Worker 2 │  │ Worker 3 │  │  Worker N    │  │  │
│  │  │ (local)  │  │ (local)  │  │ (remote) │  │  (remote)    │  │  │
│  │  │          │  │          │  │          │  │              │  │  │
│  │  └─────┬────┘  └─────┬────┘  └─────┬────┘  └──────┬───────┘  │  │
│  │        │              │              │              │          │  │
│  └────────┼──────────────┼──────────────┼──────────────┼──────────┘  │
│           │              │              │              │              │
│    ┌──────┴──────────────┴──────┐ ┌─────┴──────────────┴─────┐       │
│    │   Same A2A Interface       │ │   Same A2A Interface      │       │
│    │   (local goroutine/proc)   │ │   (remote HTTP/vsock)     │       │
│    └────────────────────────────┘ └───────────────────────────┘       │
└──────────────────────────────────────────────────────────────────────┘
          │                                        │
          ▼                                        ▼
┌─────────────────────┐              ┌──────────────────────────────┐
│  LOCAL EXECUTION    │              │   MICROVM EXECUTION          │
│                     │              │                              │
│  ┌───────────────┐  │              │  ┌────────────────────────┐  │
│  │ Agent Process  │  │              │  │   Firecracker/CHV VM   │  │
│  │               │  │              │  │                        │  │
│  │ ┌───────────┐ │  │              │  │  ┌──────────────────┐  │  │
│  │ │ MCP Server│ │  │              │  │  │  Agent Process    │  │  │
│  │ │ (tools)   │ │  │              │  │  │                  │  │  │
│  │ └───────────┘ │  │              │  │  │  ┌────────────┐  │  │  │
│  │ ┌───────────┐ │  │              │  │  │  │ MCP Server │  │  │  │
│  │ │ File I/O  │ │  │              │  │  │  │ (tools)    │  │  │  │
│  │ └───────────┘ │  │              │  │  │  └────────────┘  │  │  │
│  └───────────────┘  │              │  │  │  ┌────────────┐  │  │  │
│                     │              │  │  │  │ Sandboxed  │  │  │  │
│                     │              │  │  │  │ File I/O   │  │  │  │
│                     │              │  │  │  └────────────┘  │  │  │
│                     │              │  │  └──────────────────┘  │  │
│                     │              │  │                        │  │
│                     │              │  │  Host ← vsock → Guest  │  │
│                     │              │  └────────────────────────┘  │
└─────────────────────┘              └──────────────────────────────┘

┌──────────────────────────────────────────────────────────────────────┐
│                      LLM PROVIDER LAYER                              │
│                    (go-llm / Genkit Go)                              │
│                                                                     │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │  OpenAI  │  │ Anthropic│  │  Gemini  │  │  Ollama  │  ...       │
│  │  Adapter │  │  Adapter │  │  Adapter │  │  Adapter │            │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘            │
│                                                                     │
│  Unified interface: Chat(), Stream(), ToolCall(), StructuredOutput()│
└──────────────────────────────────────────────────────────────────────┘
```

### Key Design Decisions

1. **A2A as the universal agent interface**: Every agent (appagent and worker) exposes the same A2A-compatible API. Local agents are called in-process; remote agents are called over HTTP/vsock. The calling code doesn't change.

2. **MCP for tool access**: Each agent's capabilities (file I/O, code execution, web search, etc.) are exposed as MCP tools. This decouples the agent's reasoning from its capabilities.

3. **Actor supervision for reliability**: Proto.Actor or Ergo provides supervision trees. AppAgent supervises workers with configurable restart strategies. Failed workers are automatically restarted.

4. **Dual execution model**: Workers run either as local goroutines/processes (for trusted, lightweight tasks) or inside Firecracker/cloud-hypervisor microVMs (for untrusted code execution). The A2A interface is the same regardless.

5. **htmx + templ for the web desktop**: Server-rendered HTML with htmx for interactivity. No JavaScript framework build pipeline. SSE for streaming agent status. WebSocket for interactive sessions (terminal, chat).

6. **Canonical editing model**: App state has a single source of truth. Both users (via web UI) and appagents (via API) are canonical editors. Workers propose changes that the appagent accepts/rejects — this is enforced at the orchestration layer.
