{
  description = "rift - a tiling window manager for macOS that focuses on performance and usability";

  # Every input follows the single `nixpkgs` pin below. Left unfollowed, the
  # lock grows a SECOND nixpkgs (devshell's) plus a separate nixpkgs-lib
  # (flake-parts'): two more source trees fetched on a cold `direnv allow`
  # (~35 MiB) and a second full nixpkgs evaluation on every shell entry, for
  # packages that are already in the first one. `rust-analyzer-src` is fenix's
  # source for the rust-analyzer *package*, which this flake never builds (the
  # toolchain in nix/package.nix is cargo/rustc/rust-std/clippy/rustfmt), so
  # pointing it at nixpkgs drops that fetch outright. crane has no inputs of
  # its own; nothing to follow.
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    devshell = {
      url = "github:numtide/devshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{
      flake-parts,
      devshell,
      ...
    }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
      ];
      imports = [
        devshell.flakeModule
        (import ./nix/package.nix inputs)
        ./nix/module.nix
      ];
      perSystem =
        { ... }:
        {
          devshells.default = {
            motd = "";
          };
        };
    }
    // {
      overlays.default = import ./nix/overlay.nix inputs;
    };
}
