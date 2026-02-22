# Platform Secrets with sops-nix

## Narrative Summary (1-minute read)

ChoirOS platform API keys should be managed as encrypted data in git, then decrypted only
at NixOS activation time on the host. This runbook uses `sops-nix` + `age` so hypervisor
gets secrets through a systemd `EnvironmentFile` without relying on plaintext `.env` files.

Current production runtime is NixOS native containers (`container@sandbox-live` and
`container@sandbox-dev`). Platform keys are loaded into hypervisor only.

This configuration treats LLM, search, and resend/email keys as required platform secrets.

## What Changed

1. Added reusable NixOS module: `nix/modules/choiros-platform-secrets.nix`.
2. Added flake export: `nixosModules.choiros-platform-secrets` in `flake.nix`.
3. Added SOPS policy template: `.sops.yaml`.
4. Added secrets schema example: `infra/secrets/choiros-platform.secrets.example.yaml`.
5. Added bootstrap helper: `scripts/secrets/bootstrap-sops-nix.sh`.

## What To Do Next

1. Generate/import age keys on EC2 and operators.
2. Encrypt `infra/secrets/choiros-platform.secrets.sops.yaml` with team recipients.
3. Import module in host config and wire `services.choiros.platformSecrets`.
4. Deploy via `nixos-rebuild switch` and verify key-backed model calls.
5. Implement user-scoped secret broker endpoints as separate phase.

## Architecture Intent

- Platform secrets remain hypervisor-scoped by default.
- NixOS containers do not receive platform secrets.
- Secret values never enter repo plaintext, logs, or event payloads.

## Host Config Example

Use this in `/etc/nixos/configuration.nix` (or equivalent flake host module list):

```nix
{
  imports = [
    inputs.sops-nix.nixosModules.sops
    inputs.self.nixosModules.choiros-platform-secrets
  ];

  services.choiros.platformSecrets = {
    enable = true;
    sopsFile = /opt/choiros/deploy-repo/infra/secrets/choiros-platform.secrets.sops.yaml;
  };
}
```

This module now also renders `/run/secrets/rendered/choiros-flakehub-token` and runs a
one-shot systemd service (`choiros-flakehub-login`) that executes:

```bash
determinate-nixd login token --token-file /run/secrets/rendered/choiros-flakehub-token
```

The secret key names in the encrypted SOPS file should match runtime env names exactly:

- `AWS_BEARER_TOKEN_BEDROCK`
- `ZAI_API_KEY`
- `OPENAI_API_KEY`
- `KIMI_API_KEY`
- `MOONSHOT_API_KEY`
- `RESEND_API_KEY`
- `TAVILY_API_KEY`
- `BRAVE_API_KEY`
- `EXA_API_KEY`
- `FLAKEHUB_AUTH_TOKEN`

## One-Time Bootstrap

On the EC2 host:

```bash
sudo /opt/choiros/deploy-repo/scripts/secrets/bootstrap-sops-nix.sh
```

Then set `.sops.yaml` recipient(s), create encrypted file, and commit encrypted output:

```bash
cp infra/secrets/choiros-platform.secrets.example.yaml \
  infra/secrets/choiros-platform.secrets.sops.yaml

$EDITOR infra/secrets/choiros-platform.secrets.sops.yaml

SOPS_AGE_RECIPIENTS="age1..." \
  sops --encrypt --in-place infra/secrets/choiros-platform.secrets.sops.yaml
```

## Verification

After `nixos-rebuild switch`:

```bash
systemctl show hypervisor --property=EnvironmentFiles
systemctl status choiros-flakehub-login --no-pager
systemctl restart hypervisor
journalctl -u hypervisor -n 120 --no-pager
journalctl -u choiros-flakehub-login -n 120 --no-pager
```

Confirm model provider calls succeed from the running stack, then ensure no secret values
appear in logs.

## NixOS Container Policy

For current runtime, keep `container@sandbox-live` and `container@sandbox-dev` free of
platform provider keys. If user-level secrets are needed in sandbox execution, deliver them
through hypervisor broker APIs and scoped policy checks (ADR-0003 path).
