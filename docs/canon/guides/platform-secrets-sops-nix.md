# Platform Secrets (ADR-0008 Control-Plane Model)

Date: 2026-02-01
Kind: Guide
Status: Accepted
Requires: []

## Narrative Summary (1-minute read)

This runbook is the active operator reference for platform secrets after ADR-0008.

ChoirOS now uses a strict control-plane secret boundary:
1. No secret material in git (plaintext or encrypted).
2. No provider or user secrets in sandbox runtimes.
3. Hypervisor/control-plane services load credentials from runtime credential files.
4. `EnvironmentFile` secret injection is not allowed for production.

## What Changed

1. Replaced the old sops-in-repo flow as an active path.
2. Standardized on control-plane runtime credential delivery.
3. Required systemd credential loading (`LoadCredential`) for hypervisor/provider gateway.
4. Marked sandbox as keyless by policy for provider and search credentials.

## What To Do Next

1. Operate a self-hosted secret store for control-plane hosts.
2. Deliver provider credentials to host paths under `/run/choiros/credentials/platform/`.
3. Wire hypervisor service with `LoadCredential=` and file-based secret reads.
4. Keep sandbox/runtime env free of provider and search API keys.
5. Add integration tests proving managed-mode gateway-only behavior.

## Hard Policy Rules

1. Do not commit secrets (including encrypted secrets blobs) to this repo.
2. Do not set provider/search keys in sandbox env.
3. Do not use `Environment=` or `EnvironmentFile=` for secret values in production.
4. Do not log raw secret values.

## Host Wiring Pattern (Required)

Example systemd service shape:

```nix
systemd.services.hypervisor.serviceConfig = {
  LoadCredential = [
    "zai_api_key:/run/choiros/credentials/platform/zai_api_key"
    "kimi_api_key:/run/choiros/credentials/platform/kimi_api_key"
    "openai_api_key:/run/choiros/credentials/platform/openai_api_key"
    "inception_api_key:/run/choiros/credentials/platform/inception_api_key"
    "tavily_api_key:/run/choiros/credentials/platform/tavily_api_key"
    "brave_api_key:/run/choiros/credentials/platform/brave_api_key"
    "exa_api_key:/run/choiros/credentials/platform/exa_api_key"
  ];
};
```

Service code reads these from `$CREDENTIALS_DIRECTORY/*`.

## Runtime Policy

- Hypervisor/provider gateway holds provider credentials.
- Sandbox uses only gateway routing metadata (`CHOIR_PROVIDER_GATEWAY_*`, sandbox/user IDs).
- Managed-mode model/research calls must fail fast if gateway routing is missing.

## Verification Checklist

1. `git grep -n "choiros-platform.secrets.sops"` returns no active-path dependency.
2. Hypervisor unit config uses `LoadCredential`, not `EnvironmentFile`, for secrets.
3. Managed sandbox startup rejects provider/search key env vars.
4. Provider and search requests in managed mode succeed through gateway and fail without it.
5. Logs and events contain no secret values.

## Legacy Note

This file keeps its historical filename for link stability, but the old
sops-nix-in-repo procedure is deprecated and no longer an approved production path.
