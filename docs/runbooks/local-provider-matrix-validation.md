# Local Provider Matrix Validation

## Narrative Summary (1-minute read)

This runbook provides a repeatable local validation pass for ChoirOS provider behavior before any
host deployment work.

It validates two lanes:
1. Model provider live matrix (`sandbox/tests/model_provider_live_test.rs`)
2. Provider gateway search path (`tavily`, `brave`, `exa`) via hypervisor token boundary

Use this as a release gate for local dev and upcoming OVH bring-up.

## What Changed

1. Added `scripts/ops/validate-local-provider-matrix.sh`.
2. Standardized pass/fail checks for model + gateway search paths.
3. Added optional Codex -> `OPENAI_API_KEY` local bridge flag for dev-only ChatGPT testing.

## What To Do Next

1. Ensure local services and env are set.
2. Run the matrix script.
3. Fix any failed provider lane before changing infra/deploy surface.

## Preconditions

1. Repo root shell at `/Users/wiz/choiros-rs`.
2. Dependencies installed:
   - `cargo`
   - `curl`
   - `jq`
3. For gateway search lane:
   - hypervisor is running and reachable (default: `http://127.0.0.1:9090`)
   - `CHOIR_PROVIDER_GATEWAY_TOKEN` is set (or pass `--gateway-token`)
4. For model lane:
   - provider env keys present for selected models (for example `KIMI_API_KEY`, `ZAI_API_KEY`,
     `AWS_BEARER_TOKEN_BEDROCK`, `OPENAI_API_KEY`, etc.)

## Run the Matrix

Default run:

```bash
./scripts/ops/validate-local-provider-matrix.sh
```

Run with explicit model targets:

```bash
./scripts/ops/validate-local-provider-matrix.sh \
  --models "ZaiGLM47Flash,KimiK25"
```

Run with dev ChatGPT auth bridge (local-only):

```bash
./scripts/ops/validate-local-provider-matrix.sh \
  --models "OpenAIGPT5CodexDev,KimiK25" \
  --codex-openai-bridge
```

Gateway-only smoke:

```bash
./scripts/ops/validate-local-provider-matrix.sh \
  --skip-model-tests \
  --gateway-base http://127.0.0.1:9090 \
  --gateway-token "$CHOIR_PROVIDER_GATEWAY_TOKEN"
```

## Pass Criteria

1. `live_provider_smoke_matrix` passes for selected models.
2. `live_decide_matrix` passes for selected models.
3. Gateway search smokes return success for:
   - `gateway-search:tavily`
   - `gateway-search:brave`
   - `gateway-search:exa`
4. Script summary ends with `failures=0`.

## Common Failures

- `missing API key environment variable: ...`
  - Export missing provider key for the lane you are testing.
- `invalid provider gateway token`
  - Verify `CHOIR_PROVIDER_GATEWAY_TOKEN`.
- `status=403` for gateway search
  - Ensure upstream is in `CHOIR_PROVIDER_GATEWAY_ALLOWED_UPSTREAMS`.
- `OPENAI_API_KEY missing in .../.codex/auth.json`
  - Run `codex login`, then rerun with `--codex-openai-bridge`.

