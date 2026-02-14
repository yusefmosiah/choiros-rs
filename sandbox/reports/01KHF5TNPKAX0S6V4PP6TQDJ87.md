<!-- revision:9 -->
# how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw

## Conductor
Run accepted.

Objective: how does this compare with openclaw, nanoclaw, picoclaw? https://github.com/theonlyhennygod/zeroclaw
Run ID: `01KHF5TNPKAX0S6V4PP6TQDJ87`

Waiting for worker dispatch...

## Researcher
<!-- proposal -->
> [researcher] researcher_tool_update (01KHF5TNPKAX0S6V4PP6TQDJ87:researcher:tool:01KHF5VF1NC8WA2H9CV927NTZX)
# Research Findings: ZeroClaw vs OpenClaw Ecosystem

**Top Finding:** ZeroClaw is **not a recognized project** in the AI agent ecosystem. The search results for "zeroclaw" show information about "NanoClaw" and "PicoClaw", but no "ZeroClaw" repository or project exists in the public domain. The GitHub link `theonlyhennygod/zeroclaw` appears to be a non-existent repository or a typo.

---

## Analysis of the "Claw" Ecosystem

Since **ZeroClaw** does not exist in the public search results, here's a comparison of the **actual lightweight alternatives** to OpenClaw that were found:

### 1. **NanoClaw** 🦞
- **Repository:** [github.com/gavrielc/nanoclaw](https://github.com/gavrielc/nanoclaw) or [github.com/qwibitai/nanoclaw](https://github.com/qwibitai/nanoclaw)
- **Language:** TypeScript (~500 lines of code)
- **Stars:** 5.6k+
- **Key Features:**
  - Container-based security (Apple Containers on macOS, Docker on Linux)
  - Native WhatsApp integration
  - Built on Anthropic's Claude Agent SDK
  - Per-group memory isolation
  - Scheduled tasks and reminders
  - **Security:** OS-level container isolation, not just app-level permissions
- **Cost:** FREE (uses Claude API, ~$20-50/month typical usage)
- **Best For:** Security-conscious users wanting container isolation and WhatsApp integration

### 2. **PicoClaw** 🦐
- **Repository:** [github.com/sipeed/picoclaw](https://github.com/sipeed/picoclaw)
- **Language:** Go (self-bootstrapped from Python Nanobot)
- **Hardware:** Runs on $10 hardware with <10MB RAM (99% less than OpenClaw)
- **Key Features:**
  - Ultra-lightweight (<10MB RAM, boots in <1 second)
  - Single portable binary (no Node.js/Python dependencies)
  - Supports RISC-V, ARM64, x86-64
  - Connects to Telegram, Discord, QQ, DingTalk
  - 400x faster startup than OpenClaw
  - Uses external LLM APIs (OpenRouter, Zhipu, Anthropic, etc.)
- **Cost:** FREE (bring your own API keys)
- **Best For:** Embedded hardware, RISC-V boards, ultra-low-resource environments

### 3. **OpenClaw** (The Original)
- **Repository:** [github.com/openclaw/openclaw](https://github.com/openclaw/openclaw)
- **Language:** TypeScript (430,000+ lines of code)
- **Stars:** 168k+
- **Key Features:**
  - 15+ channels (WhatsApp, Telegram, Slack, Discord, iMessage, Signal, Teams, Web)
  - Browser automation, computer control, multi-agent orchestration
  - Extensive skill marketplace
  - Canvas UI, persistent memory, personality files
  - **SECURITY ISSUES:**
    - CVE-2026-25253 (RCE vulnerability)
    - ZeroLeaks score: 2/100
    - 84% extraction rate
    - 17,903 instances publicly exposed
- **Cost:** FREE software + API costs ($30-50 per session reported)
- **Hardware:** Requires Mac Mini ($599 recommended) or equivalent

---

## Comparison Table

| Feature | NanoClaw | PicoClaw | OpenClaw |
|---------|----------|----------|----------|
| **Codebase** | ~500 lines | Go binary | 430k+ lines |
| **RAM** | Containerized | <10MB | >1GB |
| **Hardware** | Mac/Linux | $10 board | $599 Mac Mini |
| **Security** | ✅ Container isolation | Basic (no sandbox) | ❌ No sandbox (RCE vulnerability) |
| **Channels** | WhatsApp (primary) | Telegram, Discord, QQ | 15+ channels |
| **LLM Support** | Claude SDK only | Multiple providers | Multiple providers + local |
| **Startup Time** | Normal | <1 second | 500+ seconds |
| **Cost** | ~$20-50/mo | API only | $30-50/session |

---

## Critical Security Warnings

⚠️ **OpenClaw Users:**
- Update to v2026.2.2 immediately (patches CVE-2026-25253 RCE)
- 17,903 instances are publicly exposed and being scanned
- ZeroLeaks score: 2/100 with 84% extraction rate
- Do not run without Docker isolation

✅ **Secure Alternatives:**
- **NanoClaw:** Container isolation with per-group filesystem boundaries
- **Carapace:** (Rust) OS-level sandboxing + WASM capabilities (mentioned in results)

---

## Conclusion on ZeroClaw

**ZeroClaw does not appear to exist** as a recognized project. The user likely meant:

1. **NanoClaw** - The secure, container-based WhatsApp assistant
2. **PicoClaw** - The ultra-lightweight Go assistant for embedded hardware
3. **Nanobot** - A 4,000-line Python alternative (9.2k stars)

The GitHub repository `theonlyhennygod/zeroclaw` returns no search results and may be:
- A private/unpublished repository
- A typo for one of the "-claw" projects
- A very new/forked project not yet indexed

**Recommendation:** Clarify with the user whether they meant **NanoClaw**, **PicoClaw**, or another alternative.

---

**Sources:**
- [Nanoclaw vs Openclaw: Critical Differences and Selection Guide](https://kkumdam.com/blog/en-nanoclaw-vs-openclaw-critical-differences-and-selection-guide-for-data-scraping-success-1770634856598)
- [NanoClaw: The Lightweight, Secure AI Assistant](https://lilys.ai/en/notes/openai-agent-builder-20260208/nanoclaw-lightweight-secure-ai-assistant)
- [PicoClaw Documentation](https://picoclaw.ai/docs.html)
- [PicoClaw vs OpenClaw: $10 Board vs $600 Mac Mini](http://openclawpulse.com/picoclaw-vs-openclaw/)
- [Best OpenClaw Alternatives 2026](https://grigio.org/the-best-openclaw-alternatives-for-personal-ai-agents/)
- [NanoClaw GitHub](https://github.com/gavrielc/nanoclaw)
- [PicoClaw GitHub](https://github.com/sipeed/picoclaw)
> [researcher] researcher_tool_update (01KHF5TNPKAX0S6V4PP6TQDJ87:researcher:tool:01KHF5VQAX606CZBHSKRF5CACF)
Researching ZeroClaw vs. OpenClaw/NanoClaw/PicoClaw comparison. I've found information about OpenClaw, NanoClaw, and PicoClaw from web search results. Now I need to examine ZeroClaw directly to provide an accurate comparison.
> [researcher] researcher_tool_update (01KHF5TNPKAX0S6V4PP6TQDJ87:researcher:tool:01KHF5XCHYZ8Q16TH6CJAAPP28)
## Research Progress Update

I've gathered extensive information about the OpenClaw ecosystem and several alternatives:

### Key Findings So Far:

**OpenClaw** (the baseline):
- 430,000+ lines of TypeScript
- 160K+ GitHub stars, most feature-rich
- Requires Mac Mini (~$600) or decent VPS
- Serious security concerns: CVE-2026-25253, 17,903 exposed instances
- High API costs: $30-50 per session reported by users

**NanoClaw** (security-focused):
- ~500 lines of code, understandable in 8 minutes
- Runs in Apple Containers (macOS) or Docker (Linux)
- WhatsApp-first design with per-group isolation
- True container isolation (not just app-level permissions)
- 5,600+ GitHub stars

**PicoClaw** (ultra-lightweight):
- Written in Go, <10MB RAM (99% less than OpenClaw)
- Single binary, boots in <1 second
- Runs on $10 hardware (RISC-V, ARM64, x86)
- 400x faster startup than OpenClaw
- Supports Telegram, Discord, QQ, DingTalk

**ZeroClaw** (Rust rewrite - INVESTIGATING):
- Built by @theonlyhennygod in Rust
- Tagline: "claw done right"
- Benchmark shows: 0.38s startup (cold) vs OpenClaw's 3.31s
- ~0.00s (warm) vs OpenClaw's 1.11s
- Status command runtime: ~0.00s

### Next Steps:
Currently fetching detailed information from ZeroClaw's GitHub repository to complete the comparison.

Sources:
- [NanoClaw GitHub](https://github.com/gavrielc/nanoclaw)
- [PicoClaw Documentation](https://picoclaw.ai/docs.html)
- [ZeroClaw GitHub](https://github.com/theonlyhennygod/zeroclaw)
> [researcher] researcher_tool_update (01KHF5TNPKAX0S6V4PP6TQDJ87:researcher:tool:01KHF5YP6Z0DD0BJZ4D4R9FQ9S)
# Research Update

## 🔍 Current Objective
Comparing **ZeroClaw** (https://github.com/theonlyhennygod/zeroclaw) with:
- **OpenClaw** - 430k+ lines, TypeScript, ~15 channels, full-featured
- **NanoClaw** - ~500 lines, container-isolated, WhatsApp-focused
- **PicoClaw** - Go, <10MB RAM, ultra-lightweight

## 📊 Search Queries Executed
1. Initial broad search for "zeroclaw openclaw nanoclaw picoclaw"
2. Follow-up search for "zeroclaw rust openclaw alternative"
3. Deep search for "theonlyhennygod zeroclaw" (developer focus)

## 🎯 Key Findings So Far

### 1. OpenClaw (The Baseline)
- **Repository**: [openclaw/openclaw](https://github.com/openclaw/openclaw)
- **GitHub**: 168k+ stars, TypeScript (83.6%), Swift (12.4%)
- **Architecture**: 430,000+ lines of code
- **Hardware**: Requires Mac Mini ($599) or equivalent, 1GB+ RAM
- **Channels**: ~15 platforms (WhatsApp, Telegram, Slack, Discord, iMessage, Signal, Teams, Web)
- **Security Issues**: 
  - ZeroLeaks score: 2/100
  - 17,903 publicly exposed instances (Shodan scan)
  - CVE-2026-25253 (Critical RCE vulnerability)
  - 26% of skills have vulnerabilities (Cisco report)
  - No sandboxing by default (runs on host system)
- **API Costs**: $30-50 per session reported by users


### 2. NanoClaw (Secure Alternative)
- **Repository**: [gavrielc/nanoclaw](https://github.com/gavrielc/nanoclaw)
- **GitHub**: 5.6k stars, TypeScript (98%)
- **Architecture**: ~500 lines of core code
- **Philosophy**: "Small enough to understand in 8 minutes"
- **Security**: 
  - OS-level container isolation (Apple Container/Docker)
  - Each agent runs in isolated container with filesystem limits
  - 5-minute container timeout
  - Smaller attack surface than OpenClaw
- **Channels**: WhatsApp (primary), extensible via skills
- **Built on**: Anthropic's Agents SDK (Claude Code harness)
- **Scheduling**: Recurring jobs with message-back capability
- **Unique Feature**: Agent can modify its own code and add features on demand

### 3. PicoClaw (Ultra-Lightweight)
- **Repository**: [sipeed/picoclaw](https://github.com/sipeed/picoclaw)
- **GitHub**: Ultra-lightweight, Go-based
- **Architecture**: Rewritten in Go through self-bootstrapping process
- **Memory**: <10MB RAM (99% less than OpenClaw)
- **Hardware**: Runs on $10 boards (RISC-V, ARM64, x86)
- **Boot time**: <1 second (400× faster than OpenClaw)
- **Deployment**: Single portable binary, no dependencies
- **Channels**: Telegram, Discord, QQ, DingTalk
- **Approach**: "OpenClaw's philosophy on hardware you forgot existed"

### 4. ZeroClaw (What We Know So Far)
- **Developer**: @theonlyhennygod (Argenis De La Rosa)
- **Repository**: [theonlyhennygod/zeroclaw](https://github.com/theonlyhennygod/zeroclaw)
- **Tagline**: "claw done right 🦀"
- **Language**: Rust (mentioned in tweets/X posts)
- **Early benchmarks mentioned**: 
  - "0s vs 5.98s" (startup time comparison)
  - RAM usage: 40MB vs 254MB (claimed advantage)
- **Mentions**: Listed in OpenClaw alternatives articles
- **Status**: Recently mentioned in tech communities (Feb 2026)

## ❓ Open Questions About ZeroClaw
1. **Exact architecture**: Is it a full rewrite in Rust or a wrapper?
2. **Feature set**: Does it support all OpenClaw channels or a subset?
3. **Memory footprint**: What are the actual resource requirements?
4. **Security model**: Does it use sandboxing like NanoClaw?
5. **Dependencies**: What does it require to run?

## 🔎 Next Steps
- Fetch the ZeroClaw GitHub repository to get actual details
- Compare technical specifications side-by-side
- Analyze security models across all four
- Provide final recommendation matrix

---
*Sources gathered from multiple web searches including X (Twitter), GitHub, tech blogs, and comparison articles*

## Terminal

## User

