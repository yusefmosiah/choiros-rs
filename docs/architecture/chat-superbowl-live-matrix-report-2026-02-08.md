# Chat Superbowl Live Matrix Report (No-Hint Prompt, Async Flow)

**⚠️ Architecture Notice (2026-02-09):** This document evaluates the **Chat compatibility surface** only. Per AGENTS.md Execution Direction, Chat is a thin compatibility layer that should escalate multi-step planning to **Conductor**. The findings here inform Conductor design but do not represent the target primary orchestration path.

Date: 2026-02-08  
Test target: `sandbox/tests/chat_superbowl_live_matrix_test.rs`

## Narrative Summary (1-minute read)

The no-hint Superbowl eval validates chat's async delegation behavior as a **compatibility surface**. Tests verify that Chat can delegate `web_search` to ResearcherActor without blocking, and receive clean follow-up signals. Multi-tool continuation (`web_search -> bash` chaining) is explicitly an orchestration concern that belongs in **Conductor**, not Chat's remit. Per NO ADHOC WORKFLOW policy, Chat must not implement multi-step planning logic via string matching; it escalates to Conductor for orchestration.

## What Changed

- Chat deferred-path runtime behavior was hardened:
  - scoped history is reloaded per turn so async completions persist into context,
  - deferred status messages are tagged and excluded from prompt history,
  - stale post-completion “still running” assistant chatter was removed.
- Matrix harness no longer depends on brittle status phrasing for non-blocking checks.
- Researcher `provider=auto` still defaults to parallel fanout (`tavily,brave,exa`) unless overridden.

## What To Do Next

1. **Conductor Implementation**: Add multi-tool continuation policy in Conductor (not Chat) so orchestration can route `web_search -> bash` when search evidence is insufficient.
2. Add quality/ranking guardrails for provider results before synthesis.
3. Keep Bedrock probes isolated in CI-style runs until mixed-run cert/LazyLock instability is fully explained.
4. Add explicit run-ordering assertion: deferred marker -> completion event -> final answer event.
5. **Architecture Alignment**: Ensure Chat escalates multi-step planning to Conductor per AGENTS.md; Chat remains a compatibility surface, not the canonical planner.

## Bedrock TLS Note (2026-02-09)

- Root cause pattern observed in mixed Bedrock runs:
  - intermittent `hyper-rustls` platform-cert initialization panic,
  - subsequent `LazyLock poisoned` failures in the same test process.
- Stabilization applied:
  - shared TLS cert bootstrap now sets `SSL_CERT_FILE` from known system/Nix CA bundle paths before live provider calls,
  - wired into app startup and live matrix/provider test harnesses.

## Prompt Under Test

```text
As of today, whats the weather for the superbowl?
```

No prompt-level hints for tools/providers/weather APIs were used.

## Run A (Mixed 30-case matrix)

Command:

```bash
BAML_LOG=ERROR \
CHOIR_SUPERBOWL_MATRIX_MODELS='ClaudeBedrockOpus46,ClaudeBedrockOpus45,ClaudeBedrockSonnet45,KimiK25,ZaiGLM47,ZaiGLM47Flash' \
CHOIR_SUPERBOWL_MATRIX_PROVIDERS='auto,tavily,brave,exa,all' \
CHOIR_SUPERBOWL_CASE_TIMEOUT_MS=90000 \
cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
```

Summary:

```text
SUMMARY executed=30 model_honored=true non_blocking=true signal_to_answer=true quality=true strict_passes=8 polluted_count=0 search_then_bash=true
```

Observed instability:

- Mixed Bedrock run intermittently triggered `hyper-rustls` platform-cert panic in-process, causing `LazyLock poisoned` cascades for subsequent cases.

## Run B (Clean non-Bedrock matrix, authoritative metrics)

Command:

```bash
BAML_LOG=ERROR \
CHOIR_SUPERBOWL_MATRIX_MODELS='KimiK25,ZaiGLM47,ZaiGLM47Flash' \
CHOIR_SUPERBOWL_MATRIX_PROVIDERS='auto,tavily,brave,exa,all' \
CHOIR_SUPERBOWL_CASE_TIMEOUT_MS=90000 \
cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
```

Summary:

```text
SUMMARY executed=15 model_honored=true non_blocking=true signal_to_answer=true quality=true strict_passes=8 polluted_count=0 search_then_bash=false
```

Per-case strict pass highlights:

- `KimiK25`: pass on `tavily`, `exa`, `all`; fail on `brave`; mixed on `auto`.
- `ZaiGLM47`: pass on `tavily`, `exa`, `all`; fail on `brave`; mixed on `auto`.
- `ZaiGLM47Flash`: pass on `auto`, `all`; fail on `tavily`, `brave`, `exa`.

## Run C (Isolated Bedrock probes)

Commands:

```bash
BAML_LOG=ERROR CHOIR_SUPERBOWL_MATRIX_MODELS='ClaudeBedrockOpus46' CHOIR_SUPERBOWL_MATRIX_PROVIDERS='auto' cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
BAML_LOG=ERROR CHOIR_SUPERBOWL_MATRIX_MODELS='ClaudeBedrockSonnet45' CHOIR_SUPERBOWL_MATRIX_PROVIDERS='auto' cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
BAML_LOG=ERROR CHOIR_SUPERBOWL_MATRIX_MODELS='ClaudeBedrockOpus45' CHOIR_SUPERBOWL_MATRIX_PROVIDERS='auto' cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
```

Results:

- `ClaudeBedrockOpus46`: `1/1` strict pass.
- `ClaudeBedrockSonnet45`: `1/1` strict pass.
- `ClaudeBedrockOpus45`: failed in this harness (`selected_model` empty, no tool flow), consistent with Opus45 no longer being a reliable active runtime target.

## Run D (Targeted mixed-model rerun after TLS bootstrap)

Command:

```bash
BAML_LOG=ERROR \
CHOIR_SUPERBOWL_MATRIX_MODELS='ClaudeBedrockOpus46,ClaudeBedrockSonnet45,KimiK25,ZaiGLM47' \
CHOIR_SUPERBOWL_MATRIX_PROVIDERS='auto,exa,all' \
CHOIR_SUPERBOWL_CASE_TIMEOUT_MS=90000 \
cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
```

Summary:

```text
SUMMARY executed=12 model_honored=true non_blocking=true signal_to_answer=true quality=true strict_passes=12 polluted_count=0 search_then_bash=false
```

Notes:

- Bedrock and non-Bedrock models all completed under non-blocking flow with clean follow-ups.
- No autonomous `web_search -> bash` escalation observed.

## Key Findings

- Async orchestration is now materially better:
  - background delegation does not pollute final chat context,
  - post-completion stale status messages are eliminated from prompt history.
- Model/provider quality variance remains substantial.
- No spontaneous `web_search -> bash` chain in clean matrix.
- Bedrock models can pass in isolation, but mixed-run stability still needs environment hardening.

## Current Conclusion

The architecture is now good enough to support non-blocking delegated research with clean follow-up behavior. The highest-value next step is shared harness extraction + explicit multi-tool continuation policy so capability escalation can happen reliably without object-level prompt hints.

## Run E (Post Single-Loop Chat Refactor)

Context:
- Chat refactor removed separate final synthesis call and now relies on one continuous `PlanAction` loop with deterministic fallback.
- Goal: verify non-blocking background flow remains intact and compare model/provider behavior.

Command:

```bash
cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
```

Summary:

```text
SUMMARY executed=15 model_honored=true non_blocking=true signal_to_answer=true quality=true strict_passes=8 polluted_count=0 search_then_bash=false
```

Observed per-provider behavior highlights:

- `KimiK25`
  - strong on `auto`, `brave`, `exa`, `all`
  - weaker on `tavily` (returned raw research-style output in one case)
- `ZaiGLM47Flash`
  - stalls/no-final-output on `brave`, `exa`, `all` in this run
- `ZaiGLM47`
  - solid on `exa` and `all`
  - weaker on `auto`/`tavily`, and stalled/no-final on `brave` in this run

Takeaway:
- Single-loop refactor preserved non-blocking async behavior and removed plan/synthesis split.
- Main remaining gap is not blocking/flow; it is quality policy and continuation behavior:
  - stronger source filtering/ranking before final answer,
  - explicit multi-step continuation policy to trigger `search -> terminal` escalation when search evidence is insufficient.

## Run F (Policy Escalation Contract Landed)

Context:
- Researcher now emits objective metadata (`complete|incomplete|blocked`) and recommends next capability.
- Supervisor includes policy hook for `research -> terminal` escalation when objective is incomplete.

Command:

```bash
cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
```

Summary:

```text
SUMMARY executed=15 model_honored=true non_blocking=true signal_to_answer=true quality=true strict_passes=5 polluted_count=0 search_then_bash=false
```

Notes:
- Flow guarantees held (`non_blocking=true`, `signal_to_answer=true`, `polluted_count=0`).
- Strict quality pass rate dropped in this run due model/provider variance (especially `ZaiGLM47Flash` on `brave/exa/all`).
- No autonomous `search -> bash` chain observed in this matrix run.
- This confirms policy plumbing is in place, but ranking/continuation quality remains the dominant gap.

## Run G (Async-First Research Delegation Default)

Context:
- Chat `web_search` delegation now defaults to async mode (`CHOIR_RESEARCH_DELEGATE_MODE=async`) to avoid soft-wait immediate-result pollution and keep user-facing turns non-blocking.
- Follow-up synthesis now rejects stale status-style responses and falls back to concrete tool observations.

Command:

```bash
cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
```

Summary:

```text
SUMMARY executed=15 model_honored=true non_blocking=true signal_to_answer=true quality=true strict_passes=11 polluted_count=0 search_then_bash=false
```

Notes:
- Non-blocking + completion-signal flow stayed healthy across the full run.
- Strict pass count improved vs prior clean run (`11` vs `8`) in this environment.
- Still no autonomous `web_search -> bash` chain, so continuation/escalation policy remains the next quality lever.

## Run H (Objective Planner Contract Pass, Regression Check)

Context:
- Planner output was updated to carry explicit objective state (`objective_status`, `completion_reason`).
- Chat loop completion checks were made objective-driven with compatibility fallback.
- Added evidence-first guard for verifiable/time-sensitive requests.

Command (fast regression probe):

```bash
CHOIR_SUPERBOWL_MATRIX_PROFILE=fast \
CHOIR_SUPERBOWL_MATRIX_MODELS='ZaiGLM47Flash' \
CHOIR_SUPERBOWL_MATRIX_PROVIDERS='auto' \
CHOIR_SUPERBOWL_MATRIX_MAX_CASES=1 \
cargo test -p sandbox --test chat_superbowl_live_matrix_test -- --nocapture
```

Summary:

```text
SUMMARY executed=1 model_honored=true non_blocking=false signal_to_answer=false quality=false strict_passes=0 polluted_count=0 search_then_bash=false
```

Observed symptom:
- Case reports show no `web_search`/`bash` usage and empty final answer extraction.
- Indicates live-path regression in planner/delegation flow after objective-contract changes.

Immediate next step:
- Add explicit chat planning failure-path event emission and rerun matrix to isolate whether failure is planner-call parse/runtime error vs loop/delegation suppression.
