{
  description = "Development and deployment environment for proxy.nro.run";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems =
        function:
        nixpkgs.lib.genAttrs systems (
          system:
          function (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      devShells = forAllSystems (
        pkgs:
        let
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "wasm32-unknown-unknown" ];
          };
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.actionlint
              pkgs.awscli2
              pkgs.binaryen
              pkgs.openssl
              pkgs.pkg-config
              pkgs.shellcheck
              pkgs.trunk
              pkgs.wasm-bindgen-cli
              rustToolchain
            ];
          };
        }
      );

      apps = forAllSystems (
        pkgs:
        let
          rustToolchain = pkgs.rust-bin.stable.latest.default.override {
            targets = [ "wasm32-unknown-unknown" ];
          };
          buildInputs = [
            pkgs.binaryen
            pkgs.openssl
            pkgs.pkg-config
            pkgs.trunk
            pkgs.wasm-bindgen-cli
            rustToolchain
          ];
          scriptApp =
            name: runtimeInputs: script:
            let
              package = pkgs.writeShellApplication {
                inherit name runtimeInputs;
                runtimeEnv = {
                  OPENSSL_INCLUDE_DIR = "${pkgs.lib.getDev pkgs.openssl}/include";
                  OPENSSL_LIB_DIR = "${pkgs.lib.getLib pkgs.openssl}/lib";
                  PKG_CONFIG_PATH = "${pkgs.lib.getDev pkgs.openssl}/lib/pkgconfig";
                };
                text = ''
                  exec "''${PWD}/scripts/${script}" "$@"
                '';
              };
            in
            {
              type = "app";
              program = "${package}/bin/${name}";
            };
        in
        {
          build = scriptApp "proxy-build" buildInputs "build.sh";
          check = scriptApp "proxy-check" buildInputs "check.sh";
          deploy = scriptApp "proxy-deploy" (
            buildInputs ++ [ pkgs.awscli2 ]
          ) "deploy.sh";
        }
      );

      checks = forAllSystems (pkgs: {
        repository-contract = pkgs.runCommand "proxy-repository-contract" {
          nativeBuildInputs = [
            pkgs.actionlint
            pkgs.shellcheck
          ];
        } ''
          shellcheck ${self}/scripts/*.sh
          actionlint \
            -config-file ${self}/.github/actionlint.yaml \
            ${self}/.github/workflows/*.yml
          touch "$out"
        '';
      });
    };
}
