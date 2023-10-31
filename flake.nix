{
  description = "Pemasak Handal Infrastruktur";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    crane = {
      url = "github:ipetkov/crane";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        rust-overlay.follows = "rust-overlay";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        # this is how we can tell crane to use our toolchain!
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
        src = craneLib.cleanCargoSource ./.;
        nativeBuildInputs = with pkgs; [ rustToolchain pkg-config ];
        buildInputs = with pkgs; [ openssl ];
        commonArgs = {
          inherit src buildInputs nativeBuildInputs;
        };
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;
        bin = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
        });
      in
      with pkgs;
      {
        checks = {
          inherit bin;
        };

        packages =
          {
            inherit bin;
            default = bin;

            # nix run .#dev 
            dev = pkgs.writeShellScriptBin "dev" ''
              cd "$(git rev-parse --show-toplevel)"
              RUST_LOG=info cargo run
            '';

            # nix run .#debug 
            debug = pkgs.writeShellScriptBin "dev" ''
              cd "$(git rev-parse --show-toplevel)"
              RUST_LOG=debug cargo run
            '';

            # nix run .#watch 
            watch = pkgs.writeShellScriptBin "dev" ''
              cd "$(git rev-parse --show-toplevel)"
              cargo watch -L info -x "run"
            '';
          };
        devShells.default = craneLib.devShell {
          checks = self.checks.${system};
          packages = with pkgs; [
            sqlx-cli
            bacon
            cargo-watch
            jq
            yj
          ];

          shellHook = ''
            export PATH="$PATH:$GOPATH/bin"
            ln -sf ../../scripts/pre-commit ./.git/hooks
          '';
        };
      }
    );
}
