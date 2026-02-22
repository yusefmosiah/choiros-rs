{ config, lib, pkgs, ... }:
let
  cfg = config.services.choiros.platformSecrets;
  flakehubTokenFile = config.sops.templates."choiros-flakehub-token".path;
in
{
  options.services.choiros.platformSecrets = {
    enable = lib.mkEnableOption "sops-nix managed platform API secrets for ChoirOS";

    sopsFile = lib.mkOption {
      type = lib.types.path;
      description = "Encrypted SOPS file containing ChoirOS platform secrets.";
    };

    ageKeyFile = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/sops-nix/key.txt";
      description = "Path to the age private key used by sops-nix on the host.";
    };

    hypervisorService = lib.mkOption {
      type = lib.types.str;
      default = "hypervisor";
      description = "Systemd service name for the ChoirOS hypervisor.";
    };

    flakehubLoginService = lib.mkOption {
      type = lib.types.str;
      default = "choiros-flakehub-login";
      description = "Systemd one-shot service that logs determinate-nixd into FlakeHub.";
    };

    enableFlakehubLogin = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Enable one-shot determinate-nixd login using FLAKEHUB_TOKEN from sops-nix.";
    };

  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = builtins.pathExists cfg.sopsFile;
        message = "services.choiros.platformSecrets.sopsFile must exist on the host build path.";
      }
    ];

    sops = {
      defaultSopsFile = cfg.sopsFile;
      age.keyFile = cfg.ageKeyFile;

      secrets = {
        AWS_BEARER_TOKEN_BEDROCK = { };
        ZAI_API_KEY = { };
        OPENAI_API_KEY = { };
        KIMI_API_KEY = { };
        MOONSHOT_API_KEY = { };
        RESEND_API_KEY = { };
        TAVILY_API_KEY = { };
        BRAVE_API_KEY = { };
        EXA_API_KEY = { };
        FLAKEHUB_TOKEN = { };
      };

      templates = {
        "choiros-platform.env" = {
          mode = "0400";
          content = ''
            AWS_BEARER_TOKEN_BEDROCK=${config.sops.placeholder."AWS_BEARER_TOKEN_BEDROCK"}
            ZAI_API_KEY=${config.sops.placeholder."ZAI_API_KEY"}
            OPENAI_API_KEY=${config.sops.placeholder."OPENAI_API_KEY"}
            KIMI_API_KEY=${config.sops.placeholder."KIMI_API_KEY"}
            MOONSHOT_API_KEY=${config.sops.placeholder."MOONSHOT_API_KEY"}
            RESEND_API_KEY=${config.sops.placeholder."RESEND_API_KEY"}
            TAVILY_API_KEY=${config.sops.placeholder."TAVILY_API_KEY"}
            BRAVE_API_KEY=${config.sops.placeholder."BRAVE_API_KEY"}
            EXA_API_KEY=${config.sops.placeholder."EXA_API_KEY"}
          '';
        };

        # Dedicated raw token file for determinate-nixd login.
        "choiros-flakehub-token" = {
          mode = "0400";
          content = "${config.sops.placeholder."FLAKEHUB_TOKEN"}";
        };
      };
    };

    systemd.services.${cfg.hypervisorService} = {
      serviceConfig.EnvironmentFile = [
        config.sops.templates."choiros-platform.env".path
      ];
    };

    systemd.services.${cfg.flakehubLoginService} = lib.mkIf cfg.enableFlakehubLogin {
      description = "Login determinate-nixd to FlakeHub from sops-nix token";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" "sops-nix.service" ];
      wants = [ "network-online.target" ];
      restartIfChanged = true;
      restartTriggers = [ flakehubTokenFile ];
      serviceConfig = {
        Type = "oneshot";
        User = "root";
        Group = "root";
        UMask = "0077";
        ExecStart = ''
          ${pkgs.bash}/bin/bash -euo pipefail -c '
            token_file="${flakehubTokenFile}"
            if [[ ! -s "$token_file" ]]; then
              echo "FLAKEHUB token file is missing or empty: $token_file" >&2
              exit 1
            fi
            exec determinate-nixd login token --token-file "$token_file"
          '
        '';
      };
    };
  };
}
