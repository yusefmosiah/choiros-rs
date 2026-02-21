{ config, lib, ... }:
let
  cfg = config.services.choiros.platformSecrets;
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

      };
    };

    systemd.services.${cfg.hypervisorService} = {
      serviceConfig.EnvironmentFile = [
        config.sops.templates."choiros-platform.env".path
      ];
    };
  };
}
