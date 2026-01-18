{
  inputs = {
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    {
      self,
      nixpkgs,
      utils,
      naersk,
    }:
    utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        naersk-lib = pkgs.callPackage naersk { };
      in
      {
        defaultPackage = naersk-lib.buildPackage {
          root = ./.;
          buildInputs = with pkgs; [
            openssl
            pkg-config
          ];
        };
        devShell =
          with pkgs;
          mkShell {
            buildInputs = [
              cargo
              rustc
              rustfmt
              openssl
              diesel-cli
              pre-commit
              pkg-config
              cargo-watch
              docker-compose
              rustPackages.clippy
            ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
            shellHook = ''
              export MCI_LOG_LEVEL="debug"
              export MCI_KEY_PATH="certs/key.pem"
              export MCI_CERT_PATH="certs/cert.pem"
              export MCI_DATABASE_URL="postgres://postgres:postgres@localhost:5432/mci"
              export MCI_S3_URL="http://localhost:8333"
            '';
          };
      }
    );
}
