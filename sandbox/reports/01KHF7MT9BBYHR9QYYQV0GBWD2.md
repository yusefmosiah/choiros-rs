<!-- revision:7 -->
# # how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw

ZeroClaw appears to be a new or possibly private project—the repository at https://github.com/theonlyhennygod/zeroclaw couldn't be located through multiple search attempts. The name suggests an even more minimal approach than PicoClaw, perhaps targeting zero dependencies or zero-configuration deployment, but without access to the code, its exact positioning remains speculative.

Here's how the three established variants compare, which may help contextualize where ZeroClaw aims to fit:

| Project | Language | RAM | Startup | Hardware Cost | Key Differentiator |
|---------|----------|-----|---------|---------------|-------------------|
| **OpenClaw** | TypeScript | >1GB | >500s | Mac Mini $599 | Full-featured, complex (52+ modules, multi-channel) |
| **NanoClaw** | TypeScript/Node | ~100MB | ~30s | Linux SBC ~$50 | Security via Apple containers |
| **PicoClaw** | Go | **<10MB** | **<1s** | **Any Linux $10-100** | Minimal footprint, AI-bootstrapped |

The trajectory is unmistakable: a race toward lighter, faster, cheaper. OpenClaw established the baseline with comprehensive features—WhatsApp, Telegram, Slack, Discord, Signal, iMessage, and more—but at the cost of heavy resource demands and slow startup. NanoClaw, launched January 31, 2026, traded some flexibility for security isolation, running in Apple containers and slimming down to roughly 500 lines of code. It gained significant traction with 7,000+ stars in its first week. PicoClaw, released February 9, 2026, pushed further—99% smaller than OpenClaw, running on $10 RISC-V boards, with an agent-assisted migration to Go that achieved 400× faster startup.

If you can confirm the correct ZeroClaw repository URL or provide additional details about its goals, I can refine this comparison further.

<!-- proposal -->
> [researcher] researcher_tool_update (01KHF7MT9BBYHR9QYYQV0GBWD2:researcher:tool:01KHF7NFR57PB9TB1HGRW9CKJB)
# Comparison: ZeroClaw vs OpenClaw, NanoClaw, PicoClaw

## Executive Summary

I've gathered comprehensive information about the **ZeroClaw** project and can now compare it against **OpenClaw**, **NanoClaw**, and **PicoClaw**. Here's what I found:

### ZeroClaw (theonlyhennygod/zeroclaw)
- **Status**: 🔍 **COULD NOT LOCATE** - The repository at https://github.com/theonlyhennygod/zeroclaw appears to be non-existent, private, or possibly removed.
- **Attempts**: Web searches across multiple providers (Tavily, Brave, Exa) returned no relevant results for this specific repository.
- **Uncertainty**: The repo may be newly created, renamed, or the URL might be incorrect.

## Comparison Matrix

| Project | Language | RAM | Startup | Hardware Cost | Stars | Key Differentiator |
|---------|----------|-----|---------|---------------|-------|-------------------|
| **OpenClaw** | TypeScript | >1GB | >500s | Mac Mini $599 | - | Full-featured, complex |
| **NanoClaw** | TypeScript/Node | ~100MB | ~30s | Linux SBC ~$50 | 7K+ | Security (Apple containers) |
| **PicoClaw** | Go | **<10MB** | **<1s** | **Any Linux $10-100** | 700+ | Minimal footprint, fast |
| **ZeroClaw** | Unknown | Unknown | Unknown | Unknown | N/A | **Not found** |

## Detailed Findings

### 1. OpenClaw (Baseline)
- **Complexity**: 52+ modules, 8 config files, 45+ dependencies
- **Features**: Multi-channel inbox (WhatsApp, Telegram, Slack, Discord, etc.), macOS app, WebChat
- **Issues**: Heavy resource usage, slow startup
- **Source**: [openclaw/openclaw](https://github.com/openclaw/openclaw)

### 2. NanoClaw (Security-Focused)
- **Launch**: Jan 31, 2026 (MIT License)
- **Growth**: 7,000+ stars in ~1 week
- **Approach**: Runs in **Apple containers** for security isolation
- **Code Size**: ~500 lines (slimmed down from OpenClaw)
- **Trade-off**: Containerized security vs. full resource access
- **Sources**: 
  - [gavrielc/nanoclaw](https://github.com/gavrielc/nanoclaw)
  - [VentureBeat article](https://venturebeat.com/orchestration/nanoclaw-solves-one-of-openclaws-biggest-security-issues-and-its-already)

### 3. PicoClaw (Ultra-Lightweight)
- **Launch**: Feb 9, 2026
- **Stats**: 698 stars, 74 forks
- **Breakthrough**: **99% smaller** than OpenClaw, runs on **$10 hardware**
- **Architecture**: Go-based, **AI-bootstrapped** (agent helped write its own migration)
- **Performance**: <10MB RAM, <1s boot, 400× faster startup
- **Hardware**: Runs on RISC-V, ARM64, x86 (even $10 LicheeRV-Nano)
- **Channels**: Telegram, Discord, QQ, DingTalk
- **Sources**: 
  - [sipeed/picoclaw](https://github.com/sipeed/picoclaw)
  - [Official docs](https://picoclaw.ai/docs.html)
  - [CNX Software article](https://www.cnx-software.com/2026/02/10/picoclaw-ultra-lightweight-personal-ai-assistant-run-on-just-10mb-of-ram/)

### 4. ZeroClaw (Unknown)
- **Status**: Cannot verify existence or features
- **Possible scenarios**:
  - Repository may be private or newly created
  - URL might be incorrect
  - Project might be a fork/work-in-progress not yet indexed

## Key Trends

1. **Lightweight migration**: Moving from TypeScript (OpenClaw) → Python (NanoBot) → Go (PicoClaw)
2. **Resource efficiency**: From >1GB → 100MB → 10MB RAM
3. **Hardware democratization**: From Mac Mini ($599) → Linux SBC ($50) → Any Linux ($10)
4. **Security vs. Capability trade-off**: NanoClaw chooses containerization; PicoClaw chooses minimal footprint

## Uncertainty & Next Steps

**Primary uncertainty**: ZeroClaw repository not found in searches. 

**Recommended actions**:
1. Verify the ZeroClaw repository URL
2. Check if the repo exists under a different name/organization
3. Confirm whether ZeroClaw is a fork of another project or entirely new

Sources: OpenClaw [GitHub](https://github.com/openclaw/openclaw) | NanoClaw [GitHub](https://github.com/gavrielc/nanoclaw), [VentureBeat](https://venturebeat.com/orchestration/nanoclaw-solves-one-of-openclaws-biggest-security-issues-and-its-already) | PicoClaw [GitHub](https://github.com/sipeed/picoclaw), [CNX Software](https://www.cnx-software.com/2026/02/10/picoclaw-ultra-lightweight-personal-ai-assistant-run-on-just-10mb-of-ram/)
> [researcher] researcher_tool_update (01KHF7MT9BBYHR9QYYQV0GBWD2:researcher:tool:01KHF7NQHKAYT48HA8XPQ0ACH0)
Initial findings:

**ZeroClaw**: Need to fetch the repository to understand its purpose.

**NanoClaw**: 
- Ultra-lightweight version (~500 lines) that runs in Apple containers for security
- Focus on containerized security design
- Now has Docker support for Linux users
- Explicitly described as "personal Claude assistant running in an Apple container"
- Addresses OpenClaw's security issues through sandboxing
- MIT License, launched January 31, 2026
- 7,000+ GitHub stars in first week
- Source: [gavrielc/nanoclaw](https://github.com/gavrielc/nanoclaw), [VentureBeat](https://venturebeat.com/orchestration/nanoclaw-solves-one-of-openclaws-biggest-security-issues-and-its-already)

**PicoClaw**:
- Written in Go, ultra-lightweight (<10MB RAM, 99% smaller than OpenClaw)
- Runs on $10 hardware, boots in <1 second
- Supports RISC-V, ARM64, x86 architectures
- AI-bootstrapped codebase (95% agent-generated)
- Channels: Telegram, Discord, QQ, DingTalk
- Config at ~/.picoclaw/config.json
- Source: [sipeed/picoclaw](https://github.com/sipeed/picoclaw), [Official Docs](https://picoclaw.ai/docs.html)

**OpenClaw** (original/baseline):
- TypeScript-based, full-featured personal AI assistant
- >1GB RAM, Mac Mini required (~$599)
- 52+ modules, 8 config files, 45+ dependencies
- Multi-channel inbox (WhatsApp, Telegram, Slack, Discord, Google Chat, Signal, iMessage, Teams, Matrix, etc.)
- Local-first gateway architecture
- Source: [openclaw/openclaw](https://github.com/openclaw/openclaw)

Fetching ZeroClaw details now...
> [researcher] researcher_tool_update (01KHF7MT9BBYHR9QYYQV0GBWD2:researcher:tool:01KHF7P1MR8TMW4VYJ41V48J3Z)
Research in progress. Found comprehensive information about OpenClaw, NanoClaw, and PicoClaw variants. Now fetching ZeroClaw repository details to complete the comparison analysis.

Key findings so far:
- **OpenClaw**: Full-featured TypeScript framework, multi-channel support, >1GB RAM requirement [openclaw/openclaw](https://github.com/openclaw/openclaw)
- **NanoClaw**: ~500 lines, Apple container security focus, runs on Anthropic SDK [gavrielc/nanoclaw](https://github.com/gavrielc/nanoclaw) [Medium article](https://ai-engineering-trend.medium.com/nanoclaw-a-slimmed-down-version-of-clawdbot-achieved-in-just-500-lines-of-code-c208dc16ee8f)
- **PicoClaw**: Go implementation, <10MB RAM, $10 hardware support, 400× faster startup [sipeed/picoclaw](https://github.com/sipeed/picoclaw) [Documentation](https://picoclaw.ai/docs.html)
