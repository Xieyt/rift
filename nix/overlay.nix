inputs: final: prev:
let
  # `pkgs.system` is a deprecated alias that emits an evaluation warning on
  # every consumer's rebuild; read the platform off stdenv instead.
  system = final.stdenv.hostPlatform.system;
  packageModule = (import ./package.nix inputs).perSystem {
    inherit (final) lib;
    pkgs = final;
    inherit system;
  };
in
{
  rift = packageModule.packages.rift;
  # `rift-bin` was a dangling reference: nix/package.nix names the raw-binary
  # output `rift-unwrapped`, so any consumer that touched `pkgs.rift-bin` hit an
  # "attribute missing" eval error.
  rift-unwrapped = packageModule.packages.rift-unwrapped;
}
