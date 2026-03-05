{
  description = "ChoirOS flake outputs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    microvm = {
      url = "github:microvm-nix/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, flake-utils, disko, microvm, ... }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" ];
    in
    (flake-utils.lib.eachSystem systems (_system: { }))
    // {
      nixosModules = {
        choiros-platform-secrets = import ./nix/modules/choiros-platform-secrets.nix;
      };
    }
    // {
      nixosConfigurations.choiros-vfkit-user = nixpkgs.lib.nixosSystem {
        system = "aarch64-linux";
        modules = [
          microvm.nixosModules.microvm
          ./nix/vfkit/user-vm.nix
        ];
      };

      nixosConfigurations.choiros-ovh-node = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          disko.nixosModules.disko
          ./nix/hosts/ovh-node-disk-config.nix
          ./nix/hosts/ovh-node.nix
        ];
      };

      nixosConfigurations.choiros-a = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          disko.nixosModules.disko
          ./nix/hosts/ovh-node-disk-config.nix
          ./nix/hosts/ovh-node-a.nix
        ];
      };

      nixosConfigurations.choiros-b = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        modules = [
          disko.nixosModules.disko
          ./nix/hosts/ovh-node-disk-config.nix
          ./nix/hosts/ovh-node-b.nix
        ];
      };

      # Cloud-hypervisor sandbox microVMs (x86_64-linux, run on OVH hosts)
      nixosConfigurations.choiros-ch-sandbox-live = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          sandboxRole = "live";
          sandboxPort = 8080;
          vmIp = "10.0.0.10";
          vmMac = "52:54:00:00:00:0a";
          vmTap = "tap-live";
        };
        modules = [
          microvm.nixosModules.microvm
          ./nix/ch/sandbox-vm.nix
        ];
      };

      nixosConfigurations.choiros-ch-sandbox-dev = nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs = {
          sandboxRole = "dev";
          sandboxPort = 8081;
          vmIp = "10.0.0.11";
          vmMac = "52:54:00:00:00:0b";
          vmTap = "tap-dev";
        };
        modules = [
          microvm.nixosModules.microvm
          ./nix/ch/sandbox-vm.nix
        ];
      };
    }
    ;
}
