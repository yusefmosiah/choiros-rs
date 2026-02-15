<!-- revision:5 -->
# how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw

ZeroClaw is a Rust-based reimplementation of the OpenClaw AI agent framework, trading feature breadth for dramatic performance gains. Where OpenClaw ships as a 28 MB Node.js application that takes over 3 seconds to cold-start and consumes ~1.5 GB of RAM, ZeroClaw delivers a 3.4 MB native binary that launches in under 10 milliseconds using roughly 7.8 MB of memory. This positions it as a direct alternative for production deployments where resource efficiency matters more than OpenClaw's extensive plugin ecosystem.

The broader landscape includes two minimalist alternatives that take different architectural paths. **NanoClaw** (~500 lines of Python) runs on Anthropic's Agents SDK and emphasizes radical simplicity: it sandboxes execution via Apple Containers or Docker with automatic 5-minute timeouts, communicates primarily through WhatsApp, and uniquely allows the agent to modify its own codebase to add capabilities on demand. **PicoClaw**, written in Go, offers a middle ground as a single-binary personal assistant with scheduled job support via cron, JSON-based configuration for multiple LLM providers and search backends, and broader channel coverage spanning Telegram, Discord, WhatsApp, and Feishu.

OpenClaw remains the reference point for comprehensive functionality—roughly 450,000 lines of TypeScript supporting 15+ channels, multi-agent orchestration, RAG pipelines, and extensive third-party integrations. However, this capability comes with operational overhead: 45+ dependencies, no default sandboxing, and a reported 26% vulnerability rate in its skill ecosystem according to Cisco research.

**Choosing between them depends on constraints:**

- **ZeroClaw** when you need OpenClaw-compatible behavior with production-grade efficiency
- **NanoClaw** when minimal attack surface and self-modifying agent behavior are priorities
- **PicoClaw** when you want lightweight Go deployment with flexible channel support and scheduled tasks
- **OpenClaw** when maximum integration breadth outweighs resource and security overhead

<!-- proposal -->
> [researcher] researcher_tool_update (01KHFAP0PKHYG5N15VVSNJ9RG6:researcher:tool:01KHFAP6VV7G9CZRP3S18WHKZ7)
Plan:
1. Fetch the ZeroClaw repository to understand its core purpose and architecture.
2. Perform targeted searches for 'openclaw', 'nanoclaw', and 'picoclaw' to identify and compare them against ZeroClaw.
3. Synthesize findings focusing on functionality, target use cases, and technical differences.
> [researcher] researcher_tool_update (01KHFAP0PKHYG5N15VVSNJ9RG6:researcher:tool:01KHFAPQXKJJQVNVYPSH5ATXNQ)
## ZeroClaw vs OpenClaw/NanoClaw/PicoClaw

**ZeroClaw Position**: Rust-based rewrite emphasizing performance and modularity.

### Performance Comparison (ZeroClaw vs OpenClaw)
ZeroClaw reports significantly better performance than OpenClaw [GitHub - theonlyhennygod/zeroclaw](https://github.com/theonlyhennygod/zeroclaw):
- **Binary Size**: ~3.4 MB vs 28 MB
- **Startup Time**: <10ms vs 3.31s (cold) / ~1.11s (warm)
- **Memory (RSS)**: ~7.8 MB vs ~1.52 GB (status command)
- **Build**: Rust release binary vs Node + dist build

### NanoClaw (~500 LOC)
- **Focus**: Minimalism, self-bootstrapping, agent-modifiable codebase.
- **Language**: Python (likely, given Claude Code/Agents SDK requirement).
- **Architecture**: Runs on Anthropic's Agents SDK. Uses Apple Containers (macOS) or Docker (Linux) for strict OS-level sandboxing (5-min timeout).
- **Channels**: WhatsApp (primary).
- **Security**: Smaller attack surface due to minimalism; explicit Apple Container/Docker isolation; no config file complexity.
- **Customization**: Unique capability where the agent can add features/tools to itself upon user request.

### PicoClaw (Go)
- **Focus**: Ultra-lightweight, efficient, single binary.
- **Architecture**: Ultra-lightweight personal AI assistant in Go.
- **Key Features**: Scheduled reminders/jobs via cron tool.
- **Configuration**: JSON config file for API keys (e.g., OpenRouter, Zhipu) and search (optional Brave).
- **Channels**: Multiple (Telegram, Discord, WhatsApp, Feishu, etc.).

### OpenClaw (Reference Point)
- **Language**: Node.js/TypeScript (TypeScript).
- **Scope**: Large ecosystem (~450k LOC), many integrations/skills (15+ channels, 45+ dependencies).
- **Architecture**: Multi-agent orchestration, planners, production ops, RAG pipelines.
- **Resource Usage**: High startup time, memory (~1.52 GB), larger binary.
- **Security Concerns**: No default sandbox; broad attack surface; Cisco report cites 26% skill vulnerability rate.

**Summary**: ZeroClaw targets the OpenClaw use case with Rust efficiency (binary size, startup, memory). NanoClaw and PicoClaw target minimalism and simplicity (Nano: Python agent-modifiable; Pico: Go single binary). OpenClaw is the full-featured but heavier alternative.
<!-- /proposal -->
