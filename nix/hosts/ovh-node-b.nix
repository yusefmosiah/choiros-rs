# Node B: runtime + warm standby for control plane
# 147.135.70.196 — ns106285.ip-147-135-70.us — us-east-vin
{ ... }:
{
  imports = [ ./ovh-node.nix ];
  networking.hostName = "choiros-b";
}
