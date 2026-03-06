# Node B: draft/staging server for CI rolling deploys
# 147.135.70.196 — ns106285.ip-147-135-70.us — us-east-vin
{ lib, ... }:
{
  imports = [ ./ovh-node.nix ];
  networking.hostName = "choiros-b";

  # Caddy: draft.choir-ip.com instead of choir-ip.com
  services.caddy.virtualHosts = lib.mkForce {
    "draft.choir-ip.com" = {
      extraConfig = ''
        reverse_proxy 127.0.0.1:9090
      '';
    };
  };

  # WebAuthn: draft.choir-ip.com origin for e2e tests
  systemd.services.hypervisor.serviceConfig.Environment = [
    "WEBAUTHN_RP_ID=draft.choir-ip.com"
    "WEBAUTHN_RP_ORIGIN=https://draft.choir-ip.com"
  ];
}
