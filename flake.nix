{
  inputs = {
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      fenix,
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        f = fenix.packages.${system};
        rustToolchain = f.combine [
          f.default.toolchain
          f.complete.rust-src
          f.targets.wasm32-unknown-unknown.latest.rust-std
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages = [ rustToolchain pkgs.prettier ];
        };
      }
    );
}
