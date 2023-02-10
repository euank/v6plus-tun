{
  inputs = {
    cargo2nix.url = "github:cargo2nix/cargo2nix/unstable";
    flake-utils.follows = "cargo2nix/flake-utils";
    nixpkgs.follows = "cargo2nix/nixpkgs";
  };

  outputs = inputs: with inputs;
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [cargo2nix.overlays.default];
        };

        rustPkgs = pkgs.rustBuilder.makePackageSet {
          rustVersion = "1.66.1";
          packageFun = import ./Cargo.nix;
        };

      in rec {
        packages = {
          v6plus-tun = (rustPkgs.workspace.v6plus-tun {}).bin;
          default = packages.v6plus-tun;
        };
      }
    );
}
