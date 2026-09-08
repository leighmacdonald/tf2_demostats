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
}
