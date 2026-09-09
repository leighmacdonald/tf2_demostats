{
  description = "Demo parser for Team Fortress 2";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  };

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forSystem = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system} system);
      version = (fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
      # Build the source with an explicit fileset so the package build
      # only sees the crate sources (plus Cargo.toml/lock), not demos,
      # target/ or other working-tree clutter.
      src =
        pkgs:
        pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = pkgs.lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            ./tf2_demostats
            ./tf2_demostats_cli
            ./tf2_demostats_http
          ];
        };
    in
    {
      packages = forSystem (
        pkgs: system: rec {
          tf2_demostats = pkgs.rustPlatform.buildRustPackage {
            pname = "tf2_demostats";
            inherit version;
            src = src pkgs;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
            ];
            buildInputs = with pkgs; [
              openssl
              opus # voice decoding (audiopus-sys links system libopus when found)
            ];

            # Bundled-opus fallback needs this with CMake >= 4.
            CMAKE_POLICY_VERSION_MINIMUM = "3.5";

            meta = with pkgs.lib; {
              description = "Demo parser for Team Fortress 2";
              homepage = "https://github.com/leighmacdonald/tf2_demostats";
              license = licenses.mit;
              mainProgram = "tf2_demostats";
              platforms = systems;
            };
          };
          default = tf2_demostats;
        }
      );

      devShells = forSystem (
        pkgs: system: {
          default = pkgs.mkShell {
            inputsFrom = [ self.packages.${system}.tf2_demostats ];
            hardeningDisable = [ "fortify" ];
            buildInputs = with pkgs; [
              # Rust toolchain (nix-provided, no rustup required on the host).
              # Matches the compiler used for the packaged build below.
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
              cargo-audit
              cargo-machete
              goreleaser
              zig # required by goreleaser
              just
              just-lsp
              nil
              nixd
            ];
            # Dynamically linked system libs (e.g. opus) must be findable at
            # runtime for locally built binaries run via `cargo run` / `cargo test`.
            shellHook = ''
              export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath [ pkgs.opus ]}:$LD_LIBRARY_PATH
            '';
          };
        }
      );
    };
}
