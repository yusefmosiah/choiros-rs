<!-- revision:7 -->
# how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw

## Conductor
Run accepted.

Objective: how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw
Run ID: `01KHF6RR0PC2D7E2DDV99CWXV5`

Waiting for worker dispatch...

## Researcher
<!-- proposal -->
> [researcher] researcher_tool_update (01KHF6RR0PC2D7E2DDV99CWXV5:researcher:tool:01KHF6S2H14AFXV134VDEE6928)
Plan: 
1. Fetch the ZeroClaw repository to understand its core value proposition.
2. Search for OpenClaw, NanoClaw, and PicoClaw repositories for architectural comparison.
> [researcher] researcher_tool_update (01KHF6RR0PC2D7E2DDV99CWXV5:researcher:tool:01KHF6SJHZ9RFHM7R9Q8X55110)
Research initiated on ZeroClaw vs. OpenClaw/NanoClaw/PicoClaw comparison.

**Initial findings gathered:**

**OpenClaw** ([openclaw/openclaw](https://github.com/openclaw/openclaw))
- Full-featured personal AI assistant with 52+ modules
- Multi-channel support (WhatsApp, Telegram, Discord, Slack, etc.)
- Runs as single Node.js process with shared memory
- Security: application-level (allowlists, pairing codes), not OS isolation
- Concerns: Security vulnerabilities (CVE-2026-25253, CVE-2026-25157), 18,000+ exposed instances found in scans
- Complex: 45+ dependencies, multiple config files
- Platform: Node.js, Docker support

**NanoClaw** ([gavrielc/nanoclaw](https://github.com/gavrielc/nanoclaw))
- Lightweight alternative (~500 lines core code)
- Uses Claude Agent SDK natively
- Container isolation: Apple Container (macOS) or Docker (Linux)
- Single process, filesystem isolation at OS level
- WhatsApp default, extensible via skills
- AI-native setup via Claude Code (/setup command)
- Philosophy: "Don't add features, add skills" - customization through code changes
- Platform: macOS/Linux, requires Claude Code CLI

**PicoClaw** ([sipeed/picoclaw](https://github.com/sipeed/picoclaw))
- Go-based, ultra-lightweight (~4,500 lines TypeScript in variant, also has Go implementation)
- Single binary architecture
- Support for 11+ LLM providers (OpenRouter, Anthropic, OpenAI, DeepSeek, etc.)
- Multiple channels: Telegram, Discord, DingTalk (implemented); WhatsApp, Slack, Feishu (config ready)
- Gateway server, message bus, cron tasks, heartbeat
- JSON configuration with Zod validation
- Platform: Cross-platform binaries (x86_64, ARM64, RISC-V)

**Next:** Fetching ZeroClaw repository details to complete comparison.

Sources: [OpenClaw GitHub](https://github.com/openclaw/openclaw), [NanoClaw GitHub](https://github.com/gavrielc/nanoclaw), [NanoClaw site](https://nanoclaw.net), [PicoClaw GitHub](https://github.com/sipeed/picoclaw), [PicoClaw docs](https://picoclaw.ai/docs), [Running OpenClaw in Docker](https://til.simonwillison.net/llms/openclaw-docker)
> [researcher] researcher_tool_update (01KHF6RR0PC2D7E2DDV99CWXV5:researcher:tool:01KHF6SVCC66FJ22XXHS6A0HR0)
# ZeroClaw vs. OpenClaw Variants Analysis

## Executive Summary

**ZeroClaw** is a research project claiming a simplified, alternative implementation of the OpenClaw architecture. To accurately compare it with **OpenClaw**, **NanoClaw**, and **PicoClaw**, I need to access its repository.

## Key Findings So Far

### 1. **OpenClaw** (The Baseline)
- **Architecture**: Full-featured, modular AI assistant framework.
- **Complexity**: 52+ modules, 8 config files, 45+ dependencies.
- **Features**: Multi-channel support, skill ecosystem, "ClawHub" registry.
- **Security Model**: Application-level permissions (pairing codes, allowlists). Runs as a single Node process.
- **Critique**: Has faced significant security scrutiny (e.g., CVEs regarding WebSocket bypass and command injection) [[1](https://www.reddit.com/r/MachineLearning/comments/1r30nzv/d_we_scanned_18000_exposed_openclaw_instances_and/)] [[2](https://til.simonwillison.net/llms/openclaw-docker)].

### 2. **NanoClaw** (The Security-First Fork)
- **Architecture**: Radical simplification (~500 lines of core code).
- **Isolation**: OS-level container isolation (Apple Container or Docker) for agents.
- **Philosophy**: "Small enough to understand, secure by isolation, built for one user."
- **Channel**: Defaults to WhatsApp (others via skills).
- **Goal**: Addresses OpenClaw's security concerns by restricting filesystem access via containers [[3](https://github.com/gavrielc/nanoclaw)].

### 3. **PicoClaw** (The Go Rewrite for Portability)
- **Architecture**: Rewritten in Go for a single, self-contained binary.
- **Target**: Efficient for edge devices (e.g., Raspberry Pi) and simple deployment.
- **Focus**: Ultra-lightweight, low-resource footprint.
- **Status**: There appear to be two distinct projects named PicoClaw (by Sipeed and Mosaxiv). The Sipeed version is a Go rewrite for efficiency [[4](https://picoclaw.ai/docs)] [[5](https://github.com/sipeed/picoclaw)] [[6](https://github.com/mosaxiv/picoclaw)].

## Next Steps: Investigating ZeroClaw

I am now fetching the ZeroClaw repository to determine its specific innovations and how it fits into this landscape. I will update this document with a direct comparison once its code is analyzed.

## Terminal

## User

