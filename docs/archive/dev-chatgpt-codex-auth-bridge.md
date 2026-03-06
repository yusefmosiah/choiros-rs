# Dev-Only ChatGPT (Codex) Auth Bridge for ChoirOS

## Narrative Summary (1-minute read)

This runbook documents how to use your personal ChatGPT/Codex login as a **development-only**
provider path while building ChoirOS itself. The goal is local iteration speed for "build Choir
in Choir" workflows, not production serving.

The implementation path is:

1. Sign in with Codex (`codex login`) so Codex manages OAuth + token refresh.
2. Read the local `OPENAI_API_KEY` value from `~/.codex/auth.json` into shell env.
3. Point Choir at a local model-catalog override using `provider = "openai-generic"`.
4. Use that model for selected callsites during development.

Production remains API-key billing and separate auth.

## What Changed

1. Added a concrete runbook for local ChatGPT/Codex auth bridging into Choir model routing.
2. Added explicit guardrails to prevent accidental production use.
3. Added verification and rollback commands for quick local switching.

## What To Do Next

1. Create your local model-catalog override file and select callsites.
2. Export `OPENAI_API_KEY` from Codex auth storage into your shell.
3. Run the live provider smoke test against your local OpenAI model entry.
4. Keep this flow local-only; use paid API auth for shared/prod workloads.

## Scope and Guardrails

- Local development only.
- Do not commit personal tokens or local auth files.
- Do not rely on this flow for unattended server-side workloads.
- If tokens fail/expire, re-auth through Codex and refresh shell env.

## Prerequisites

- `codex` CLI installed locally.
- `jq` installed.
- Choir repo checked out locally at `/Users/wiz/choiros-rs`.

## Step 1: Authenticate Codex

Browser flow:

```bash
codex login
```

Headless/device flow:

```bash
codex login --device-auth
```

Verify:

```bash
codex login status
```

Expected status: logged in using ChatGPT.

## Step 2: Export Dev Token into Shell

Codex stores auth state in `${CODEX_HOME:-$HOME/.codex}/auth.json`. Export the key into your
current shell session:

```bash
export OPENAI_API_KEY="$(
  jq -r '.OPENAI_API_KEY // empty' "${CODEX_HOME:-$HOME/.codex}/auth.json"
)"
test -n "$OPENAI_API_KEY" || { echo "OPENAI_API_KEY missing in Codex auth.json"; return 1; }
```

Optional helper for repeated use:

```bash
refresh_codex_openai_key() {
  export OPENAI_API_KEY="$(
    jq -r '.OPENAI_API_KEY // empty' "${CODEX_HOME:-$HOME/.codex}/auth.json"
  )"
  test -n "$OPENAI_API_KEY"
}
```

## Step 3: Create a Local Choir Model Catalog Override

Do not edit production/shared catalog for this. Use a local override file:

```bash
cp sandbox/config/model-catalog.toml sandbox/config/model-catalog.dev-chatgpt.toml
```

In `sandbox/config/model-catalog.dev-chatgpt.toml`, add a model entry:

```toml
[models.OpenAIGPT5CodexDev]
name = "OpenAI GPT-5 Codex (Dev via Codex login)"
provider = "openai-generic"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
model = "gpt-5-codex"
aliases = ["OpenAIGPT5CodexDev"]
```

Then include it in `allowed_models` and set callsite defaults you want to route through it, for
example:

```toml
allowed_models = [
  # existing models...
  "OpenAIGPT5CodexDev"
]

[callsite_defaults]
conductor = "OpenAIGPT5CodexDev"
writer = "OpenAIGPT5CodexDev"
terminal = "OpenAIGPT5CodexDev"
```

Use only the callsites you want to test; keep blast radius small while iterating.

## Step 4: Activate the Override

```bash
export CHOIR_MODEL_CONFIG_PATH="/Users/wiz/choiros-rs/sandbox/config/model-catalog.dev-chatgpt.toml"
```

Optional one-off override:

```bash
export CHOIR_DEFAULT_MODEL="OpenAIGPT5CodexDev"
```

## Step 5: Verify in Choir

Run exact live-provider test binary against your dev model:

```bash
CHOIR_LIVE_MODEL_IDS=OpenAIGPT5CodexDev \
cargo test -p sandbox --test model_provider_live_test live_provider_smoke_matrix -- --nocapture
```

Then run your normal dev stack:

```bash
just dev-sandbox
just dev-ui
```

## Troubleshooting

- `missing API key environment variable: OPENAI_API_KEY`
  - Re-run export in Step 2.
- `unknown model: OpenAIGPT5CodexDev`
  - Confirm model id/alias spelling and `allowed_models` entry.
- 401/403 from upstream
  - Re-run `codex login` (or `codex login --device-auth`), then refresh env.
- Unexpected fallback model routing
  - Check `CHOIR_MODEL_CONFIG_PATH` is set in the same shell running Choir.
  - Check callsite defaults in the dev catalog file.

## Rollback to Normal Local Config

```bash
unset CHOIR_MODEL_CONFIG_PATH
unset CHOIR_DEFAULT_MODEL
unset OPENAI_API_KEY
```

If needed, restart the shell and run with default `sandbox/config/model-catalog.toml`.
