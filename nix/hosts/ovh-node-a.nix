# Node A: production (choir-ip.com)
# 51.81.93.94 — ns1004307.ip-51-81-93.us — us-east-vin
{ ... }:
{
  imports = [ ./ovh-node.nix ];
  networking.hostName = "choiros-a";

  systemd.services.hypervisor.serviceConfig.Environment = [
    "WEBAUTHN_RP_ID=choir-ip.com"
    "WEBAUTHN_RP_ORIGIN=https://choir-ip.com"
  ];
}
