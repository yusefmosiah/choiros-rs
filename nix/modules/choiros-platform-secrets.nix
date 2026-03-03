{ config, lib, ... }:
let
  cfg = config.services.choiros.platformSecrets;

  mkCredentialEntry = name: path:
    if path == null || path == "" then null else "${name}:${path}";

  loadCredentialEntries = builtins.filter (v: v != null) [
    (mkCredentialEntry "zai_api_key" cfg.credentialPaths.ZAI_API_KEY)
    (mkCredentialEntry "kimi_api_key" cfg.credentialPaths.KIMI_API_KEY)
    (mkCredentialEntry "openai_api_key" cfg.credentialPaths.OPENAI_API_KEY)
    (mkCredentialEntry "inception_api_key" cfg.credentialPaths.INCEPTION_API_KEY)
    (mkCredentialEntry "tavily_api_key" cfg.credentialPaths.TAVILY_API_KEY)
    (mkCredentialEntry "brave_api_key" cfg.credentialPaths.BRAVE_API_KEY)
    (mkCredentialEntry "exa_api_key" cfg.credentialPaths.EXA_API_KEY)
  ];
in
{
  options.services.choiros.platformSecrets = {
    enable = lib.mkEnableOption "control-plane credential wiring for ChoirOS (ADR-0008)";

    hypervisorService = lib.mkOption {
      type = lib.types.str;
      default = "hypervisor";
      description = "Systemd service name for the ChoirOS hypervisor.";
    };

    credentialPaths = {
      ZAI_API_KEY = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Absolute host path to Z.ai API key file.";
      };
      KIMI_API_KEY = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Absolute host path to Kimi API key file.";
      };
      OPENAI_API_KEY = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Absolute host path to OpenAI API key file.";
      };
      INCEPTION_API_KEY = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Absolute host path to Inception Labs API key file.";
      };
      TAVILY_API_KEY = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Absolute host path to Tavily API key file.";
      };
      BRAVE_API_KEY = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Absolute host path to Brave API key file.";
      };
      EXA_API_KEY = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Absolute host path to Exa API key file.";
      };
    };

    flakehubAuthTokenPath = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Absolute host path to FlakeHub auth token file (optional).";
    };

    flakehubLoginService = lib.mkOption {
      type = lib.types.str;
      default = "choiros-flakehub-login";
      description = "Systemd one-shot service that logs determinate-nixd into FlakeHub.";
    };

    enableFlakehubLogin = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable one-shot determinate-nixd login using flakehubAuthTokenPath.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = loadCredentialEntries != [ ];
        message = "services.choiros.platformSecrets: at least one provider credential path must be configured.";
      }
    ] ++ lib.optionals (cfg.enableFlakehubLogin && cfg.flakehubAuthTokenPath == null) [
      {
        assertion = false;
        message = "services.choiros.platformSecrets.flakehubAuthTokenPath is required when enableFlakehubLogin=true.";
      }
    ];

    systemd.services.${cfg.hypervisorService}.serviceConfig.LoadCredential = loadCredentialEntries;

    systemd.services.${cfg.flakehubLoginService} = lib.mkIf cfg.enableFlakehubLogin {
      description = "Login determinate-nixd to FlakeHub from runtime credential file";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      restartIfChanged = true;
      restartTriggers = [ cfg.flakehubAuthTokenPath ];
      script = ''
        set -euo pipefail
        if ! command -v determinate-nixd >/dev/null 2>&1; then
          echo "determinate-nixd not found on PATH; skipping FlakeHub login"
          exit 0
        fi
        token_file='${cfg.flakehubAuthTokenPath}'
        if [[ ! -s "$token_file" ]]; then
          echo "FlakeHub token file is missing or empty: $token_file" >&2
          exit 1
        fi
        exec determinate-nixd auth login token --token-file "$token_file"
      '';
      serviceConfig = {
        Type = "oneshot";
        User = "root";
        Group = "root";
        UMask = "0077";
      };
    };
  };
}
