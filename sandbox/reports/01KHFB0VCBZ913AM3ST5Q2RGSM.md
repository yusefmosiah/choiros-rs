<!-- revision:9 -->
#  how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw

ZeroClaw is a Rust-based reimagining of the OpenClaw architecture that trades the original's feature breadth for dramatic efficiency gains. According to benchmark data from its README (February 2026, macOS arm64), ZeroClaw ships as a 3.4MB binary—8.2× smaller than OpenClaw's 28MB distribution—and cold-starts in 0.38 seconds versus OpenClaw's 3.31 seconds. The memory difference is starker still: ZeroClaw sits at roughly 7.8MB RSS while OpenClaw consumes ~1.52GB, a 195× reduction. Simple commands that take OpenClaw nearly six seconds execute effectively instantly on ZeroClaw.

Where OpenClaw built a mature ecosystem with 50+ integrations, mobile apps for iOS and Android, voice wake, and a gateway control plane, ZeroClaw pursues a different philosophy. Its 22+ providers and eight trait-based subsystems emphasize explicit allowlists, workspace scoping, and pluggable components across providers, channels, tools, memory, tunnels, and observers. The codebase includes 1,017 tests and offers a migration path for existing OpenClaw users.

Two other variants occupy distinct niches:

- **NanoClaw** strips the concept to roughly 500 lines of TypeScript, prioritizing security auditability and OS-level container isolation via Apple Containers or Docker. Built on the Anthropic Agent SDK, it treats the codebase as AI-managed and supports agent swarms—garnering 8,300+ GitHub stars in its first week.

- **PicoClaw**, written in Go, targets extreme constraints: it runs on $10 hardware with under 10MB RAM, boots in under a second, and supports x86_64, ARM64, and RISC-V. Despite its size, it retains cron scheduling, web search, memory, and multi-channel support including Telegram, Discord, QQ, and DingTalk.

| | ZeroClaw | OpenClaw | NanoClaw | PicoClaw |
|--|----------|----------|----------|----------|
| **Language** | Rust | TypeScript/Node.js | TypeScript | Go |
| **Size** | 3.4MB binary | 28MB dist | ~500 LOC source | <10MB RAM runtime |
| **Startup** | ~0.4s cold | ~3.3s cold | N/A | <1s boot |
| **Memory** | ~8MB | ~1.5GB | N/A | <10MB |
| **Sweet spot** | Production servers, Rust safety | Full-featured assistant, broad integrations | Security-critical, AI-native workflow | Embedded, low-cost hardware |

Choose ZeroClaw when you need OpenClaw-like capabilities without the resource overhead—particularly for server deployments where startup time and memory footprint directly affect costs. OpenClaw remains the default for personal use across devices. NanoClaw suits environments where every line of code must be auditable and OS-level isolation matters. PicoClaw fits where hardware costs matter more than convenience.

<!-- proposal -->
> [researcher] researcher_tool_update (01KHFB0VCBZ913AM3ST5Q2RGSM:researcher:tool:01KHFB1FZGPRNJBRDRY5XGD9NH)
# ZeroClaw vs OpenClaw/NanoClaw/PicoClaw Comparison

## Key Findings

**ZeroClaw** is a Rust-based reimagining of the OpenClaw architecture, focusing on performance and security through trait-based design.

### Performance Comparison (ZeroClaw vs OpenClaw)
From ZeroClaw's README benchmark data [Feb 2026, macOS arm64]:
- **Binary size**: ZeroClaw 3.4MB vs OpenClaw 28MB (8.2× smaller)
- **Startup time**: ZeroClaw 0.38s cold vs OpenClaw 3.31s (8.7× faster)
- **Memory (status)**: ZeroClaw ~7.8MB RSS vs OpenClaw ~1.52GB RSS (195× smaller)
- **Command runtime**: ZeroClaw ~0ms vs OpenClaw 5.98s

Source: [ZeroClaw GitHub](https://github.com/theonlyhennygod/zeroclaw)

### Architectural Comparison

**ZeroClaw** (Rust, ~3.4MB):
- 100% Rust, 22+ providers, 8 trait-based subsystems
- Explicit allowlists, workspace scoping, pairing
- 1,017 tests, migration path from OpenClaw
- Pluggable: providers, channels, tools, memory, tunnels, observers

**OpenClaw** (Node.js, ~28MB dist):
- Mature ecosystem: 50+ integrations, 9+ categories, 12.5K GitHub stars
- Full-featured: macOS/iOS/Android apps, WebChat, voice wake, PTT
- Gateway control plane, multi-channel inbox
- Higher resource footprint

**NanoClaw** (TypeScript, ~500 LOC):
- Minimalist: ~500 lines of core TypeScript
- Security-first: OS-level container isolation (Apple Containers/Docker)
- "Skills over features" philosophy - AI-managed codebase
- Built on Anthropic Agent SDK, supports agent swarms
- 7,000+ GitHub stars in first week

**PicoClaw** (Go, <10MB RAM):
- Ultra-lightweight: runs on $10 hardware, <10MB RAM
- <1s boot time, 400× faster startup than OpenClaw
- Multi-arch: x86_64, ARM64, RISC-V support
- Full capabilities: cron, web search, memory, channels (Telegram, Discord, QQ, DingTalk)

### Tradeoffs Summary

| Dimension | ZeroClaw | OpenClaw | NanoClaw | PicoClaw |
|-----------|----------|----------|----------|----------|
| Language | Rust | TypeScript | TypeScript | Go |
| Binary/Size | 3.4MB | 28MB dist | ~500 LOC | <10MB RAM |
| Startup | <0.4s | ~3.3s | N/A | <1s |
| Memory | ~7-8MB | ~1.5GB | N/A | <10MB |
| Philosophy | Traits, pluggable, secure | Feature-rich, mature | Minimal, auditable | Ultra-portable |
| Target | Servers/desktops | Desktop/mobile/cloud | Security-critical | Embedded/low-cost |

### Recommendations
- **Choose ZeroClaw for**: Production servers requiring low memory, fast startup, Rust safety, and architectural flexibility
- **Choose OpenClaw for**: Full-featured personal assistant with mobile apps and broadest integration ecosystem
- **Choose NanoClaw for**: Maximum security auditability, OS-level isolation, AI-native development workflow
- **Choose PicoClaw for**: Extreme resource constraints, embedded hardware, RISC-V/ARM64 platforms
> [researcher] researcher_tool_update (01KHFB0VCBZ913AM3ST5Q2RGSM:researcher:tool:01KHFB1Y8P3RFH74JX17F9CHTW)
## Comparison: ZeroClaw vs OpenClaw/NanoClaw/PicoClaw

### Core Architectural Differences

**ZeroClaw** (Rust)
- **Architecture**: Trait-based plugin system where *everything* is swappable (providers, channels, memory, tools, tunnels, runtime, security policies) via config only
- **Security**: Pairing-based, workspace scoping, allowlists, explicit encryption - but runs on native host (same attack surface as OpenClaw)
- **Performance**: ~3.4MB binary, <10ms startup, ~7-8MB RSS (99% less than OpenClaw)
- **Design Philosophy**: "Zero lock-in" - OpenAI-compatible providers, fully agnostic, deploy anywhere (Mac/Linux/Pi)
- **Migration**: Includes `zeroclaw migrate openclaw` tool

**OpenClaw** (TypeScript/Node)
- **Architecture**: Monolithic Gateway + microservices architecture
- **Performance**: 28MB dist, 3.31s cold start, ~394MB-1.52GB RSS
- **Ecosystem**: 50+ integrations, 9 categories, most mature
- **Security Concerns**: App-level allowlists only, runs single Node process with shared memory

**NanoClaw** (TypeScript)
- **Architecture**: Minimal (~500 LOC), built on Anthropic Agent SDK
- **Security**: **OS-level isolation** - Apple Containers (macOS) or Docker (Linux) - strict filesystem sandboxing
- **Philosophy**: "Skills over Features" - AI-native setup via `/add-telegram` skills, no config sprawl
- **Tradeoff**: WhatsApp-only by default (though extensible via skills)

**PicoClaw** (Go)
- **Architecture**: Ultra-lightweight Go rewrite
- **Performance**: <10MB RAM (99% smaller), <1s boot, runs on $10 hardware (RISC-V/ARM/x86)
- **Hardware**: Targets embedded devices (LicheeRV Nano, NanoKVM, MaixCAM)
- **Design**: Single binary, true portability across architectures

### Key Tradeoffs Summary

| Aspect | ZeroClaw | OpenClaw | NanoClaw | PicoClaw |
|--------|----------|----------|----------|----------|
| **Security Model** | App-level (pairing/allowlists) | App-level only | **OS Containers** | App-level |
| **Resource Usage** | ~7-8MB RSS | 394MB-1.52GB | Container overhead | <10MB RAM |
| **Startup Time** | <10ms | 3.31s cold | Container spawn | <1s |
| **Extensibility** | **Plugin traits** | Modules | Skills via Claude | Config-driven |
| **Deployment Target** | Native hosts | Server/Mac | Container hosts | **$10 hardware** |
| **Code Complexity** | Rust (compiled) | ~400K LOC TypeScript | ~500 LOC TypeScript | Go (binary) |
| **Primary Use Case** | Agnostic, portable infrastructure | Full-featured assistant | Secure, simple, auditable | Embedded/IoT |

### Key Insight
- **ZeroClaw** dominates on **architecture flexibility** (trait-based plugins) and **performance** (Rust speed, tiny footprint) while maintaining security through pairing/scoping
- **NanoClaw** dominates on **security isolation** (OS containers) and **auditability** (500 LOC) but sacrifices breadth (WhatsApp-focused)
- **PicoClaw** dominates on **resource efficiency** (<10MB, $10 hardware) and **portability** (single binary, 3 architectures)
- **OpenClaw** remains the **feature king** but with significant resource/security tradeoffs

### Sources
- [ZeroClaw README](https://github.com/theonlyhennygod/zeroclaw) - benchmark comparisons, architecture docs
- [NanoClaw README](https://github.com/gavrielc/nanoclaw) - container isolation, philosophy
- [PicoClaw README](https://github.com/sipeed/picoclaw) - hardware specs, Go rewrite
- [NanoClaw Security Analysis](https://novalogiq.com/2026/02/11/nanoclaw-solves-one-of-openclaws-biggest-security-issues-and-its-already-powering-the-creators-biz/) - container security deep dive
- [OpenClaw GitHub](https://github.com/openclaw/openclaw) - architecture reference
- [PicoClaw Documentation](https://picoclaw.ai/docs.html) - deployment scenarios

### Uncertainties
- NanoClaw's exact memory overhead from container runtime not explicitly quantified in sources
- ZeroClaw's security model details (pairing protocol encryption specifics) require deeper source review
- PicoClaw's feature parity with OpenClaw channels not fully detailed
> [researcher] researcher_tool_update (01KHFB0VCBZ913AM3ST5Q2RGSM:researcher:tool:01KHFB2EDCBGHVB9D5XV7K3PGV)
## Comparison Summary: ZeroClaw vs OpenClaw, NanoClaw, PicoClaw

### Core Architecture Differences

**ZeroClaw (Rust)**
- **Language**: 100% Rust
- **Design**: "claw done right" - zero overhead, zero compromise
- **Size**: ~3.4MB binary (vs 28MB OpenClaw)
- **Startup**: <10ms cold start (vs 3.31s OpenClaw)
- **Memory**: ~7.3-7.8 MB RSS (vs 394MB-1.52GB OpenClaw)
- **Architecture**: Trait-based plugin system (8 traits: Provider, Channel, Memory, Tool, Observer, Runtime, Security, Tunnel)
- **Security**: Gateway pairing, strict sandboxing, explicit allowlists, workspace scoping
- **Providers**: 22+ (OpenRouter, Anthropic, OpenAI, Ollama, etc.)
- **Channels**: CLI, Telegram, Discord, Slack, iMessage, Matrix, WhatsApp, Webhook
- **License**: MIT
- **Source**: [ZeroClaw GitHub](https://github.com/theonlyhennygod/zeroclaw)

**OpenClaw (TypeScript/Node)**
- **Language**: TypeScript/Node.js
- **Size**: ~28MB dist (3.4GB+ with dependencies)
- **Startup**: 3.31s cold start
- **Memory**: 394MB - 1.52GB RSS
- **Architecture**: "permissionless" architecture - runs directly on host
- **Security**: Application-level allowlists, pairing codes (no OS isolation)
- **Channels**: 13+ platforms (WhatsApp, Telegram, Slack, Discord, iMessage, etc.)
- **License**: MIT
- **GitHub**: [openclaw/openclaw](https://github.com/openclaw/openclaw)

**NanoClaw (TypeScript)**
- **Language**: TypeScript (~500 lines core)
- **Design**: Minimal, auditable codebase
- **Architecture**: Single-process Node.js orchestrator with per-group message queues
- **Security**: **Container-isolated** (Apple Containers on macOS, Docker on Linux) - OS-level filesystem isolation
- **Channels**: WhatsApp-focused (main), extensible via skills
- **Philosophy**: "Skills over features" - no feature bloat
- **Features**: Agent Swarms support (via Anthropic Agent SDK)
- **License**: MIT
- **GitHub**: [qwibitai/nanoclaw](https://github.com/qwibitai/nanoclaw)
- **Stars**: 8.3k (fastest growing)

**PicoClaw (Go)**
- **Language**: Go (AI-bootstrapped implementation)
- **Design**: Ultra-lightweight for embedded/$10 hardware
- **Size**: <10MB RAM footprint (99% smaller than OpenClaw)
- **Startup**: <1 second boot (400× faster than OpenClaw)
- **Hardware**: Runs on $10 Linux boards (RISC-V, ARM, x86)
- **Architecture**: Single binary, true portability
- **Channels**: Telegram, Discord, QQ, DingTalk (configurable)
- **Providers**: OpenRouter, Zhipu, Anthropic, OpenAI, DeepSeek, Groq
- **License**: View license (in repo)
- **GitHub**: [sipeed/picoclaw](https://github.com/sipeed/picoclaw)
- **Stars**: 7.7k

### Key Comparative Insights

1. **Security Model**:
   - **NanoClaw**: Strongest - OS-level container isolation
   - **ZeroClaw**: Gateway pairing + sandboxing + allowlists
   - **PicoClaw**: Minimal attack surface (small codebase)
   - **OpenClaw**: Application-level only (weakest)

2. **Resource Efficiency**:
   - **PicoClaw**: Best for embedded (<10MB RAM)
   - **ZeroClaw**: Excellent (3.4MB, <10ms startup)
   - **NanoClaw**: Good (Node.js but minimal)
   - **OpenClaw**: Heaviest (1.5GB+ RAM)

3. **Extensibility**:
   - **ZeroClaw**: Most flexible (8 trait-based plugin points)
   - **OpenClaw**: Most channels/modules (but bloated)
   - **NanoClaw**: Skills-based (AI-driven customization)
   - **PicoClaw**: Fixed feature set (embedded-focused)

4. **Use Case Fit**:
   - **ZeroClaw**: Production-ready, secure, full-featured
   - **NanoClaw**: Security-first, auditable, personal deployments
   - **PicoClaw**: Edge/embedded, $10 hardware, resource-constrained
   - **OpenClaw**: Feature-rich but resource-heavy

### Superseded Claims
- *Earlier search suggested NanoClaw was just a lightweight clone* - **CORRECTED**: NanoClaw's key innovation is OS-level container isolation, not just code size.

### Sources
- ZeroClaw README & benchmarks: [github.com/theonlyhennygod/zeroclaw](https://github.com/theonlyhennygod/zeroclaw)
- NanoClaw architecture & philosophy: [github.com/qwibitai/nanoclaw](https://github.com/qwibitai/nanoclaw)
- PicoClaw embedded capabilities: [github.com/sipeed/picoclaw](https://github.com/sipeed/picoclaw)
- OpenClaw overview: [github.com/openclaw/openclaw](https://github.com/openclaw/openclaw)
- NanoClaw security analysis: [VentureBeat article](https://venturebeat.com/orchestration/nanoclaw-solves-one-of-openclaws-biggest-security-issues-and-its-already)
- PicoClaw documentation: [picoclaw.ai/docs.html](https://picoclaw.ai/docs.html)
<!-- /proposal -->

<!-- proposal -->
> [user] human_prompt (01KHFB0VCBZ913AM3ST5Q2RGSM:user:prompt:01KHFB28EXF4PCNFS7245CYJ20)
Apply the following user-authored document edits as intent for the next revision.
Treat this diff as the human prompt and update the narrative accordingly.

Removed:
- <!-- revision:1 -->
- #  how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw
- 
- This draft will become a coherent comparison based on incoming evidence.
- The run has started and the researcher is gathering source-backed findings.
- Objective:  how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw
- Run ID: `01KHFB0VCBZ913AM3ST5Q2RGSM`

Added:
+ #  how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw
+ simplify to 3 bullet points
+ ZeroClaw is a Rust-based reimagining of the OpenClaw architecture that trades the original's feature breadth for dramatic efficiency gains. According to benchmark data from its README (February 2026, macOS arm64), ZeroClaw ships as a 3.4MB binary—8.2× smaller than OpenClaw's 28MB distribution—and cold-starts in 0.38 seconds versus OpenClaw's 3.31 seconds. The memory difference is starker still: ZeroClaw sits at roughly 7.8MB RSS while OpenClaw consumes ~1.52GB, a 195× reduction. Simple commands that take OpenClaw nearly six seconds execute effectively instantly on ZeroClaw.
+ 
+ Where OpenClaw built a mature ecosystem with 50+ integrations, mobile apps for iOS and Android, voice wake, and a gateway control plane, ZeroClaw pursues a different philosophy. Its 22+ providers and eight trait-based subsystems emphasize explicit allowlists, workspace scoping, and pluggable components across providers, channels, tools, memory, tunnels, and observers. The codebase includes 1,017 tests and offers a migration path for existing OpenClaw users.
+ Two other variants occupy distinct niches. NanoClaw strips the concept to roughly 500 lines of TypeScript, prioritizing security auditability and OS-level container isolation via Apple Containers or Docker. Built on the Anthropic Agent SDK, it treats the codebase as AI-managed and supports agent swarms—garnering 7,000+ GitHub stars in its first week. PicoClaw, written in Go, targets extreme constraints: it runs on $10 hardware with under 10MB RAM, boots in under a second, and supports x86_64, ARM64, and RISC-V. Despite its size, it retains cron scheduling, web search, memory, and multi-channel support including Telegram, Discord, QQ, and DingTalk.
+ 
+ | | ZeroClaw | OpenClaw | NanoClaw | PicoClaw |
+ |--|----------|----------|----------|----------|
+ | **Language** | Rust | TypeScript/Node.js | TypeScript | Go |
+ | **Size** | 3.4MB binary | 28MB dist | ~500 LOC source | <10MB RAM runtime |
+ | **Startup** | ~0.4s cold | ~3.3s cold | N/A | <1s boot |
+ | **Memory** | ~8MB | ~1.5GB | N/A | <10MB |
+ | **Sweet spot** | Production servers, Rust safety | Full-featured assistant, broad integrations | Security-critical, AI-native workflow | Embedded, low-cost hardware |
+ 
+ Choose ZeroClaw when you need OpenClaw-like capabilities without the resource overhead—particularly for server deployments where startup time and memory footprint directly affect costs. OpenClaw remains the default for personal use across devices. NanoClaw suits environments where every line of code must be auditable. PicoClaw fits where hardware costs matter more than convenience.
+ 
+ <!-- proposal -->
+ > [researcher] researcher_tool_update (01KHFB0VCBZ913AM3ST5Q2RGSM:researcher:tool:01KHFB1FZGPRNJBRDRY5XGD9NH)
+ # ZeroClaw vs OpenClaw/NanoClaw/PicoClaw Comparison
+ 
+ ## Key Findings
+ 
+ **ZeroClaw** is a Rust-based reimagining of the OpenClaw architecture, focusing on performance and security through trait-based design.
+ 
+ ### Performance Comparison (ZeroClaw vs OpenClaw)
+ From ZeroClaw's README benchmark data [Feb 2026, macOS arm64]:
+ - **Binary size**: ZeroClaw 3.4MB vs OpenClaw 28MB (8.2× smaller)
+ - **Startup time**: ZeroClaw 0.38s cold vs OpenClaw 3.31s (8.7× faster)
+ - **Memory (status)**: ZeroClaw ~7.8MB RSS vs OpenClaw ~1.52GB RSS (195× smaller)
+ - **Command runtime**: ZeroClaw ~0ms vs OpenClaw 5.98s
+ 
+ Source: [ZeroClaw GitHub](https://github.com/theonlyhennygod/zeroclaw)
+ 
+ ### Architectural Comparison
+ 
+ **ZeroClaw** (Rust, ~3.4MB):
+ - 100% Rust, 22+ providers, 8 trait-based subsystems
+ - Explicit allowlists, workspace scoping, pairing
+ - 1,017 tests, migration path from OpenClaw
+ - Pluggable: providers, channels, tools, memory, tunnels, observers
+ 
+ **OpenClaw** (Node.js, ~28MB dist):
+ - Mature ecosystem: 50+ integrations, 9+ categories, 12.5K GitHub stars
+ - Full-featured: macOS/iOS/Android apps, WebChat, voice wake, PTT
+ - Gateway control plane, multi-channel inbox
+ - Higher resource footprint
+ 
+ **NanoClaw** (TypeScript, ~500 LOC):
+ - Minimalist: ~500 lines of core TypeScript
+ - Security-first: OS-level container isolation (Apple Containers/Docker)
+ - "Skills over features" philosophy - AI-managed codebase
+ - Built on Anthropic Agent SDK, supports agent swarms
+ - 7,000+ GitHub stars in first week
+ 
+ **PicoClaw** (Go, <10MB RAM):
+ - Ultra-lightweight: runs on $10 hardware, <10MB RAM
+ - <1s boot time, 400× faster startup than OpenClaw
+ - Multi-arch: x86_64, ARM64, RISC-V support
+ - Full capabilities: cron, web search, memory, channels (Telegram, Discord, QQ, DingTalk)
+ 
+ ### Tradeoffs Summary
+ 
+ | Dimension | ZeroClaw | OpenClaw | NanoClaw | PicoClaw |
+ |-----------|----------|----------|----------|----------|
+ | Language | Rust | TypeScript | TypeScript | Go |
+ | Binary/Size | 3.4MB | 28MB dist | ~500 LOC | <10MB RAM |
+ | Startup | <0.4s | ~3.3s | N/A | <1s |
+ | Memory | ~7-8MB | ~1.5GB | N/A | <10MB |
+ | Philosophy | Traits, pluggable, secure | Feature-rich, mature | Minimal, auditable | Ultra-portable |
+ | Target | Servers/desktops | Desktop/mobile/cloud | Security-critical | Embedded/low-cost |
+ 
+ ### Recommendations
+ - **Choose ZeroClaw for**: Production servers requiring low memory, fast startup, Rust safety, and architectural flexibility
+ - **Choose OpenClaw for**: Full-featured personal assistant with mobile apps and broadest integration ecosystem
+ - **Choose NanoClaw for**: Maximum security auditability, OS-level isolation, AI-native development workflow
+ - **Choose PicoClaw for**: Extreme resource constraints, embedded hardware, RISC-V/ARM64 platforms
+ > [researcher] researcher_tool_update (01KHFB0VCBZ913AM3ST5Q2RGSM:researcher:tool:01KHFB1Y8P3RFH74JX17F9CHTW)
+ ## Comparison: ZeroClaw vs OpenClaw/NanoClaw/PicoClaw
+ 
+ ### Core Architectural Differences
+ 
+ **ZeroClaw** (Rust)
+ - **Architecture**: Trait-based plugin system where *everything* is swappable (providers, channels, memory, tools, tunnels, runtime, security policies) via config only
+ - **Security**: Pairing-based, workspace scoping, allowlists, explicit encryption - but runs on native host (same attack surface as OpenClaw)
+ - **Performance**: ~3.4MB binary, <10ms startup, ~7-8MB RSS (99% less than OpenClaw)
+ - **Design Philosophy**: "Zero lock-in" - OpenAI-compatible providers, fully agnostic, deploy anywhere (Mac/Linux/Pi)
+ - **Migration**: Includes `zeroclaw migrate openclaw` tool
+ 
+ **OpenClaw** (TypeScript/Node)
+ - **Architecture**: Monolithic Gateway + microservices architecture
+ - **Performance**: 28MB dist, 3.31s cold start, ~394MB-1.52GB RSS
+ - **Ecosystem**: 50+ integrations, 9 categories, most mature
+ - **Security Concerns**: App-level allowlists only, runs single Node process with shared memory
+ 
+ **NanoClaw** (TypeScript)
+ - **Architecture**: Minimal (~500 LOC), built on Anthropic Agent SDK
+ - **Security**: **OS-level isolation** - Apple Containers (macOS) or Docker (Linux) - strict filesystem sandboxing
+ - **Philosophy**: "Skills over Features" - AI-native setup via `/add-telegram` skills, no config sprawl
+ - **Tradeoff**: WhatsApp-only by default (though extensible via skills)
+ 
+ **PicoClaw** (Go)
+ - **Architecture**: Ultra-lightweight Go rewrite
+ - **Performance**: <10MB RAM (99% smaller), <1s boot, runs on $10 hardware (RISC-V/ARM/x86)
+ - **Hardware**: Targets embedded devices (LicheeRV Nano, NanoKVM, MaixCAM)
+ - **Design**: Single binary, true portability across architectures
+ 
+ ### Key Tradeoffs Summary
+ 
+ | Aspect | ZeroClaw | OpenClaw | NanoClaw | PicoClaw |
+ |--------|----------|----------|----------|----------|
+ | **Security Model** | App-level (pairing/allowlists) | App-level only | **OS Containers** | App-level |
+ | **Resource Usage** | ~7-8MB RSS | 394MB-1.52GB | Container overhead | <10MB RAM |
+ | **Startup Time** | <10ms | 3.31s cold | Container spawn | <1s |
+ | **Extensibility** | **Plugin traits** | Modules | Skills via Claude | Config-driven |
+ | **Deployment Target** | Native hosts | Server/Mac | Container hosts | **$10 hardware** |
+ | **Code Complexity** | Rust (compiled) | ~400K LOC TypeScript | ~500 LOC TypeScript | Go (binary) |
+ | **Primary Use Case** | Agnostic, portable infrastructure | Full-featured assistant | Secure, simple, auditable | Embedded/IoT |
+ 
+ ### Key Insight
+ - **ZeroClaw** dominates on **architecture flexibility** (trait-based plugins) and **performance** (Rust speed, tiny footprint) while maintaining security through pairing/scoping
+ - **NanoClaw** dominates on **security isolation** (OS containers) and **auditability** (500 LOC) but sacrifices breadth (WhatsApp-focused)
+ - **PicoClaw** dominates on **resource efficiency** (<10MB, $10 hardware) and **portability** (single binary, 3 architectures)
+ - **OpenClaw** remains the **feature king** but with significant resource/security tradeoffs
+ 
+ ### Sources
+ - [ZeroClaw README](https://github.com/theonlyhennygod/zeroclaw) - benchmark comparisons, architecture docs
+ - [NanoClaw README](https://github.com/gavrielc/nanoclaw) - container isolation, philosophy
+ - [PicoClaw README](https://github.com/sipeed/picoclaw) - hardware specs, Go rewrite
+ - [NanoClaw Security Analysis](https://novalogiq.com/2026/02/11/nanoclaw-solves-one-of-openclaws-biggest-security-issues-and-its-already-powering-the-creators-biz/) - container security deep dive
+ - [OpenClaw GitHub](https://github.com/openclaw/openclaw) - architecture reference
+ - [PicoClaw Documentation](https://picoclaw.ai/docs.html) - deployment scenarios
+ 
+ ### Uncertainties
+ - NanoClaw's exact memory overhead from container runtime not explicitly quantified in sources
+ - ZeroClaw's security model details (pairing protocol encryption specifics) require deeper source review
+ - PicoClaw's feature parity with OpenClaw channels not fully detailed
+ <!-- /proposal -->
+  to
<!-- /proposal -->
