#{ lib, ... }:
let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-26.05";

  pkgs = import nixpkgs {
    config = { };
    overlays = [ ];
  };
in
pkgs.mkShell {
  #  LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.openssl ];
  hardeningDisable = [ "fortify" ];
  buildInputs = with pkgs; [
    pkg-config
    openssl
    opus # voice decoding (steam-audio-codec links system libopus when found)
    cmake
    zlib
    libgit2
    rust-analyzer
    cargo-audit
    goreleaser
    zig # required by goreleaser
    just
    just-lsp
    nil
    nix
    nixd
    cargo-machete
    rustup
  ];
  # Dynamically linked system libs (e.g. opus) must be findable at runtime
  # for locally built binaries run via `cargo run` / `cargo test`.
  shellHook = ''
    export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath [ pkgs.opus ]}:$LD_LIBRARY_PATH
  '';
}
