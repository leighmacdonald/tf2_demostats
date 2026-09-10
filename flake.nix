{
  description = "Demo parser for Team Fortress 2";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      fenix,
      ...
    }:
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
          default =
            let
              # Hermetic Rust toolchain (no rustup required), including the
              # target stds used for release builds (`windows-gnu`; gnu is
              # the host target and always included, listed for clarity).
              toolchain =
                with fenix.packages.${system};
                combine [
                  (stable.withComponents [
                    "cargo"
                    "clippy"
                    "rustc"
                    "rustfmt"
                  ])
                  targets.x86_64-unknown-linux-gnu.stable.rust-std
                  targets.x86_64-pc-windows-gnu.stable.rust-std
                ];
              # Only the static thread archives, symlinked into one dir.
              # (The packages also ship .dll.a import libs, which must NOT be
              # visible, or the exe would gain DLL dependencies at runtime.)
              mingw-thread-libs = pkgs.runCommand "mingw-thread-libs" { } ''
                mkdir -p $out/lib
                ln -s ${pkgs.pkgsCross.mingwW64.windows.mcfgthreads}/lib/libmcfgthread.a $out/lib/
                ln -s ${pkgs.pkgsCross.mingwW64.windows.pthreads}/lib/libpthread.a $out/lib/
              '';
              # `x86_64-w64-mingw32-gcc` wrapper: drops any cargo-style
              # `--target=` flag the `cc` crate may append and adds the
              # thread-lib search path.
              mingw-cc-wrapper = pkgs.writeShellScriptBin "x86_64-w64-mingw32-gcc" ''
                args=()
                for a in "$@"; do
                  case "$a" in
                    --target=*) ;;
                    *) args+=("$a") ;;
                  esac
                done
                exec ${pkgs.pkgsCross.mingwW64.buildPackages.gcc}/bin/x86_64-w64-mingw32-gcc \
                  -L${mingw-thread-libs}/lib "''${args[@]}"
              '';
            in
            pkgs.mkShell {
              hardeningDisable = [ "fortify" ];
              buildInputs = [
                toolchain
                # MinGW cross toolchain for Windows (`windows-gnu`) builds,
                # e.g. `goreleaser release --snapshot`. The compiler wrapper
                # must precede binutils on PATH so it (not the raw compiler)
                # is picked up; the thread libs it references stay out of
                # LIBRARY_PATH so no Windows objects can leak into native
                # links.
                mingw-cc-wrapper
                pkgs.pkgsCross.mingwW64.buildPackages.binutils
                # C build deps for `cargo build` (these also feed the
                # `nix build` package via the same inputs).
                pkgs.pkg-config
                pkgs.cmake
                pkgs.openssl
                pkgs.opus
              ]
              ++ (with pkgs; [
                rust-analyzer
                cargo-audit
                cargo-machete
                goreleaser
                just
                just-lsp
                nil
                nixd
              ]);
              # MinGW cross toolchain selection for the Windows target
              # (used by cargo, the `cc` crate, and cmake alike).
              CC_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-gcc";
              CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "x86_64-w64-mingw32-gcc";
              AR_x86_64_pc_windows_gnu = "x86_64-w64-mingw32-ar";
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
