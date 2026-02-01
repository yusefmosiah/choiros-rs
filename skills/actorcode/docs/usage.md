# Actorcode Usage

## Install

```bash
cd skills/actorcode
npm install
```

## Environment

```bash
export OPENCODE_SERVER_URL=http://localhost:4096
export OPENCODE_SERVER_USERNAME=opencode
export OPENCODE_SERVER_PASSWORD=your_password
```

## Allowed models (fast to pricey)

Tier labels (fast to pricey):

1. pico: `zai-coding-plan/glm-4.7-flash`
2. nano: `zai-coding-plan/glm-4.7`
3. micro: `opencode/kimi-k2.5-free`
4. milli: `openai/gpt-5.2-codex`

Capabilities:

- pico: Text-only. Run scripts/tools and quick research; not for writing new code.
- nano: Text-only. Coding-capable worker for straightforward changes.
- micro: Multimodal (text+image). General-purpose, resource-efficient default.
- milli: Multimodal (text+image). Long-context + debugging heavy lifting.

If `--model` is omitted, actorcode defaults to `pico`.

## Spawn a session

```bash
node skills/actorcode/scripts/actorcode.js spawn \
  --title "auth-research" \
  --agent explore \
  --tier pico \
  --prompt "Audit the auth flow and summarize risks."
```

## Check status

```bash
node skills/actorcode/scripts/actorcode.js status
```

## List models

```bash
node skills/actorcode/scripts/actorcode.js models
```

## Send a message

```bash
node skills/actorcode/scripts/actorcode.js message \
  --to <session_id> \
  --text "Focus on the session cookie path and expiry."
```

## Tail events

```bash
node skills/actorcode/scripts/actorcode.js events --session <session_id>
```

## Tail logs

```bash
node skills/actorcode/scripts/actorcode.js logs --id <session_id>
```

## Attach to server

```bash
node skills/actorcode/scripts/actorcode.js attach --
```
