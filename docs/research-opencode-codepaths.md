# OpenCode TUI vs Headless API Codepath Investigation

## Problem Statement

Kimi For Coding API works when connected via OpenCode TUI (`/connect` → paste API key), but fails when using headless API with model string `kimi-for-coding/k2p5`. The error is:

```
Kimi For Coding is currently only available for Coding Agents such as Kimi CLI, Claude Code, Roo Code, Kilo Code, etc.
```

This suggests different codepaths for TUI connection vs headless API usage.

## Current State

**Working:**
- TUI connection via `/connect` → "Other" → provider ID `kimi-for-coding` → paste API key
- Manual curl with `User-Agent: claude-code/1.0` header works
- OpenCode logs show request goes to `https://api.kimi.com/coding/v1/chat/completions`

**Not Working:**
- Headless API via `client.session.promptAsync()` with model `kimi-for-coding/k2p5`
- Same error even with `opencode.json` headers configuration
- Error occurs in `@ai-sdk/openai-compatible` provider

## Key Files/Areas to Investigate

1. **OpenCode Provider SDK** (`@ai-sdk/openai-compatible`)
   - How does it handle headers?
   - Does it pass custom headers from config?
   - Location: `~/.config/opencode/node_modules/@ai-sdk/openai-compatible/`

2. **OpenCode Provider Loading**
   - How are providers loaded from `opencode.json`?
   - Does TUI use different provider initialization than API?
   - Check: `opencode debug config` output differences

3. **OpenCode Session/LLM Service**
   - How does `session.promptAsync()` differ from TUI message sending?
   - Look for: `service=llm`, `service=session.processor` in logs
   - Check if TUI adds special headers or metadata

4. **User-Agent Handling**
   - Where is User-Agent set in OpenCode?
   - Does TUI override it differently than API?
   - Check if there's hardcoded User-Agent for certain providers

5. **Auth/Credential Flow**
   - How does TUI store credentials vs API usage?
   - Check `~/.local/share/opencode/auth.json` structure
   - Does TUI add metadata that API doesn't?

## Research Tasks

1. **Find OpenCode source/provider code**
   - Search for `@ai-sdk/openai-compatible` implementation
   - Look for header handling in provider SDK
   - Check if headers from config are actually passed

2. **Compare TUI vs API request flow**
   - Look for differences in how TUI sends messages vs API
   - Check if TUI uses a different provider instance
   - Look for any special handling for "connected" providers

3. **Identify the fix**
   - Determine what TUI does that API doesn't
   - Find where to add User-Agent header properly
   - Or find if we need to use a different provider/model string

## Resolution ✅

**Found:** TUI uses `@ai-sdk/anthropic` provider internally, overriding the configured npm package.

**Fix:** Change `opencode.json` to use `@ai-sdk/anthropic` instead of `@ai-sdk/openai-compatible`:

```json
{
  "provider": {
    "kimi-for-coding": {
      "npm": "@ai-sdk/anthropic",
      ...
    }
  }
}
```

**Verified:** Headless API now works with `kimi-for-coding/k2p5` model.

## Expected Findings

- [x] Location where User-Agent is set in OpenCode
- [x] Why headers from opencode.json aren't being passed
- [x] Difference between TUI provider initialization and API provider usage
- [x] Working solution for headless API usage

## Context

- Repository: `/Users/wiz/choiros-rs`
- OpenCode config: `/Users/wiz/choiros-rs/opencode.json`
- OpenCode auth: `~/.local/share/opencode/auth.json`
- Working manual curl: `curl -H "User-Agent: claude-code/1.0" https://api.kimi.com/coding/v1/chat/completions`
- Error log shows: `providerID=kimi-for-coding pkg=@ai-sdk/openai-compatible`

Report findings with [LEARNING] <category>: <description>
Mark completion with [COMPLETE]
