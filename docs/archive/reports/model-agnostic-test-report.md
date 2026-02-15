# Model-Agnostic Test Report

Date: 2026-02-08
Scope: Phase A/B validation for runtime model routing in Chat/Terminal harness + UX surfacing
Owner: sandbox backend

## Executive Summary

Phase A/B is passing for harness logic and websocket integration:
- Runtime model resolution logic is validated (request/app/env/fallback + alias support).
- Chat model override is validated at actor and HTTP API boundaries.
- Terminal delegation path accepts and forwards model override fields.
- WebSocket model-switch and actor-call stream tests pass in unrestricted environment.
- UI now surfaces `model_used` and `model_source` in assistant bundle metadata.

Live provider coverage is now validated for all runtime-configured models in `ModelRegistry`.

## Test Matrix

| Area | Test/Command | Result | Notes |
|---|---|---|---|
| Build | `cargo check -p sandbox` | PASS | Compiles with new model-routing paths |
| Test compile | `cargo check -p sandbox --tests` | PASS | Test targets compile |
| Model resolution priority | `cargo test -p sandbox --lib actors::model_config::tests:: -- --nocapture` | PASS | 9 tests passed |
| Chat model switching | `cargo test -p sandbox --lib actors::chat_agent::tests::test_model_switching -- --nocapture` | PASS | canonical + alias validation |
| Chat per-request invalid override | `cargo test -p sandbox --lib actors::chat_agent::tests::test_per_request_model_override_validation -- --nocapture` | PASS | invalid override rejected deterministically |
| HTTP model override parsing | `cargo test -p sandbox --test chat_api_test test_send_message_accepts_model_override -- --nocapture` | PASS | `/chat/send` accepts `model` field |
| Chat actor baseline tool list | `cargo test -p sandbox --test tools_integration_test test_agent_get_available_tools -- --nocapture` | PASS | regression safety check |
| WS full integration suite | `cargo test -p sandbox --test websocket_chat_test -- --nocapture` | PASS | 19 tests passed |
| Frontend compile (model metadata UX) | `cargo check --manifest-path dioxus-desktop/Cargo.toml` | PASS | model metadata wiring compiles |
| Live provider calls (Bedrock/Z.ai/Kimi) | `SSL_CERT_FILE=/etc/ssl/cert.pem cargo test -p sandbox --test model_provider_live_test -- --nocapture` | PASS | 9/9 configured runtime models passed live |
| WS actor-call streaming regression | `cargo test -p sandbox --test websocket_chat_test -- --nocapture` | PASS | 19/19 passed, includes delegated terminal actor-call streaming tests |

## New/Updated Tests Included In Phase A

### Added/updated in code
- `sandbox/src/actors/model_config.rs`
  - Added env-default and precedence tests
  - Added legacy-alias registry-creation test
  - Added model-resolution source string mapping test
  - Added env-mutex guard for deterministic env-var tests
- `sandbox/src/actors/chat_agent.rs`
  - `test_per_request_model_override_validation`
  - Added `model_source` propagation + model selection/model changed event logging
- `sandbox/tests/chat_api_test.rs`
  - `test_send_message_accepts_model_override`
- `sandbox/tests/model_provider_live_test.rs`
  - `live_provider_smoke_matrix` (live QuickResponse smoke matrix across configured providers)
- `sandbox/src/api/websocket_chat.rs`
  - Added `model_source` field to streamed `response` payload
- `dioxus-desktop/src/components.rs`
  - Assistant bundle now stores/displays `model_used` + `model_source`

## Findings

1. Model-routing core is stable in-process.
- `request > app > user > env > fallback` is validated.
- legacy aliases (`ClaudeBedrock`, `GLM47`) map to canonical model IDs.

2. API boundary accepts runtime override and websocket streaming preserves model metadata.
- HTTP `/chat/send` accepts `model` and returns success.
- WS suite passes and includes model-switch pathways.

3. Test reliability fix required.
- Env-based tests initially raced due global env var mutation.
- Resolved with mutex guard and env restore.

## Live Provider Results (Credentialed Run)

Executed on 2026-02-08 with environment visible to test process.

Command:
- `SSL_CERT_FILE=/etc/ssl/cert.pem cargo test -p sandbox --test model_provider_live_test -- --nocapture`

Result summary:
- attempted=9 passed=9 failed=0

Passing models:
- `ClaudeBedrockHaiku45`
- `ClaudeBedrockOpus45`
- `ClaudeBedrockOpus46`
- `ClaudeBedrockSonnet45`
- `KimiK25`
- `KimiK25Fallback`
- `ZaiGLM47`
- `ZaiGLM47Air`
- `ZaiGLM47Flash`

Important configuration findings from live probing:
- Z.ai works with BAML via `anthropic` provider at `https://api.z.ai/api/anthropic` and lowercase model IDs (`glm-4.7`, `glm-4.7-flash`, `glm-4.5-air`).
- Kimi coding path requires custom header support in client options; runtime config now supports provider-level headers and sets:
  - `User-Agent: claude-code/1.0` for Kimi coding models.

## Known Gaps (Remaining for Full Provider Gate)

1. Provider-specific tool-calling behavior
- This pass validates live `QuickResponse` inference only.
- Plan/tool-call functions should be validated per provider in a dedicated live matrix (next phase).

## Recommended Provider Smoke Commands (Credentialed Environment)

```bash
# Live provider smoke tests (with creds configured)
cargo test -p sandbox --test model_provider_live_test -- --nocapture

# WS integration regression after provider changes
cargo test -p sandbox --test websocket_chat_test -- --nocapture
```

## Gate Recommendation

Phase A/B status: **PASSED for harness + WS + UX surfacing**.
Provider certification status: **PASS for all currently configured runtime models (live inference matrix green)**.

Pass conditions met:
- Core model routing logic and request-boundary plumbing are validated.

Outstanding before full provider certification:
- Add live `PlanAction` + tool-call provider matrix and enforce in CI with opt-in credentials.
