{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        deps = with pkgs; [
          openssl
          pkg-config
          postgresql
        ];
        rustVersion = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "clippy"
            "rust-src"
            "rust-analyzer"
          ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs =
            deps
            ++ (with pkgs; [
              rustVersion
              diesel-cli
              cargo-watch
              docker-compose
            ]);
          shellHook = ''
            export PKG_CONFIG_PATH="${pkgs.openssl.dev}/lib/pkg-config"
          '';
        };
      }
    );
}
