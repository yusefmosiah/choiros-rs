{
  description = "ChoirOS flake outputs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    microvm = {
      url = "github:microvm-nix/microvm.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, flake-utils, microvm, ... }:
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
    }
    ;
}
