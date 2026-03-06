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
    "HYPERVISOR_PORT=9090"
    "HYPERVISOR_DATABASE_URL=sqlite:/opt/choiros/data/hypervisor.db"
    "SANDBOX_VFKIT_CTL=/opt/choiros/bin/ovh-runtime-ctl"
    "SANDBOX_LIVE_PORT=8080"
    "SANDBOX_DEV_PORT=8081"
    "FRONTEND_DIST=/opt/choiros/workspace/dioxus-desktop/target/dx/dioxus-desktop/release/web/public"
    "WEBAUTHN_RP_ID=draft.choir-ip.com"
    "WEBAUTHN_RP_ORIGIN=https://draft.choir-ip.com"
  ];
}
