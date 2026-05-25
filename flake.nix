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
        manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
        rustToolchain = f.combine [
          f.default.toolchain
          f.complete.rust-src
          f.targets.wasm32-unknown-unknown.latest.rust-std
        ];
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
        vdf-fmt = rustPlatform.buildRustPackage {
          pname = "vdf-fmt";
          version = manifest.version;

          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;

          meta = {
            description = "Opinionated KeyValues/VDF formatter";
            homepage = "https://github.com/imkunet/vdf-fmt";
            license = pkgs.lib.licenses.bsd0;
            mainProgram = "vdf-fmt";
          };
        };
        app = {
          type = "app";
          program = "${vdf-fmt}/bin/vdf-fmt";
          meta.description = "Run vdf-fmt";
        };
      in
      {
        packages = {
          default = vdf-fmt;
          vdf-fmt = vdf-fmt;
        };

        apps = {
          default = app;
          vdf-fmt = app;
        };

        devShells.default = pkgs.mkShell {
          packages = [ rustToolchain pkgs.prettier ];
        };
      }
    );
}
