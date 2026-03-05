# Node A: primary ingress + control plane + runtime
# 51.81.93.94 — ns1004307.ip-51-81-93.us — us-east-vin
{ ... }:
{
  imports = [ ./ovh-node.nix ];
  networking.hostName = "choiros-a";
}
