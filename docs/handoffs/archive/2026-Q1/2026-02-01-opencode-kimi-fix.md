# Handoff: OpenCode Kimi Provider Fix

**Date:** 2026-02-01  
**Session:** OpenCode TUI vs Headless API Investigation  
**Status:** ✅ COMPLETE - Critical Fix Applied

## Summary

Fixed the headless API issue preventing `kimi-for-coding/k2p5` from working via Actorcode. The micro tier now functions correctly.

## The Problem

- **TUI mode:** `/connect` → Other → providerID "kimi-for-coding" → WORKED
- **Headless API:** `client.session.promptAsync()` with `kimi-for-coding/k2p5` → FAILED
  - Error: "Kimi For Coding is currently only available for Coding Agents such as Kimi CLI, Claude Code, Roo Code, Kilo Code, etc."

## Root Cause

Through actorcode research sessions, discovered that:
- **TUI codepath** uses `@ai-sdk/anthropic` provider internally (hardcoded override)
- **Headless API** uses whatever npm package is configured in `opencode.json`
- The config had `@ai-sdk/openai-compatible` which doesn't send the proper User-Agent headers

Evidence from logs:
```
TUI:   providerID=kimi-for-coding pkg=@ai-sdk/anthropic using bundled provider
API:   providerID=kimi-for-coding pkg=@ai-sdk/openai-compatible using bundled provider
```

## The Fix

Changed `opencode.json`:

```diff
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "kimi-for-coding": {
-     "npm": "@ai-sdk/openai-compatible",
+     "npm": "@ai-sdk/anthropic",
      "name": "Kimi For Coding",
      "options": {
        "baseURL": "https://api.kimi.com/coding/v1",
        "headers": {
          "User-Agent": "claude-code/1.0"
        }
      },
      ...
    }
  }
}
```

## Verification

Tested with actorcode micro tier:
- Session created successfully: `ses_3e5776e0effeBh8sjbdHVA0k4A`
- Response received: "test successful"
- No 403 "Coding Agents" error

## Impact

- ✅ Actorcode micro tier (`kimi-for-coding/k2p5`) now works via headless API
- ✅ Can spawn agents with all tiers: pico, nano, micro, milli
- ✅ Enables full research automation with Kimi For Coding subscription

## Files Modified

1. `/Users/wiz/choiros-rs/opencode.json` - Changed npm package to `@ai-sdk/anthropic`
2. `/Users/wiz/choiros-rs/opencode.json.backup` - Original config preserved
3. `/Users/wiz/choiros-rs/progress.md` - Documented the fix
4. `/Users/wiz/choiros-rs/docs/research-opencode-codepaths.md` - Updated with resolution

## Next Steps

1. **Commit and push** these changes
2. **Resume actorcode research** - all tiers now functional
3. **Consider Choir-native approach** - OpenCode proved to have undocumented provider overrides

## Research Sessions

- `ses_3e59b9994ffe1ns6LcjG4WiI4Y` - Initial investigation (micro tier, failed as expected)
- `ses_3e5992a0bffe4Awkv4NE5dKQvd` - Nano tier research (found config/config divergence)
- `ses_3e58ebe46ffeSHbmjqYUY4i49s` - Source trace (CRITICAL: found TUI uses `@ai-sdk/anthropic`)
- `ses_3e5776e0effeBh8sjbdHVA0k4A` - Fix verification (SUCCESS)

## Key Learnings

1. OpenCode TUI has hardcoded provider overrides that don't respect `opencode.json`
2. `@ai-sdk/anthropic` works with Kimi API despite being an "OpenAI-compatible" endpoint
3. Actorcode research system successfully traced and fixed the issue
4. Choir's explicit actor model will be more reliable than OpenCode's hidden overrides

---

**Next Session:** Resume Choir development with working actorcode orchestration
