{
  crane,
  fenix,
  ...
}:
{
  perSystem =
    {
      pkgs,
      lib,
      system,
      ...
    }:
    let
      # Slim toolchain: just what's needed to build, lint, and format. Drops
      # rust-docs (~705M), rust-analyzer, and rust-src from the fenix "kitchen
      # sink" stable.toolchain. Shared by both the crane build and the devShell
      # so nothing is duplicated in the store.
      #
      # rustfmt comes from the NIGHTLY channel (`latest.rustfmt`) on purpose:
      # `rustfmt.toml` turns on unstable options (overflow_delimited_expr,
      # brace_style, fn_single_line, where_single_line, ...) that STABLE rustfmt
      # silently ignores. A stable `cargo fmt` then reflows the entire tree to
      # stable defaults — a huge spurious cross-file diff. Nightly rustfmt honours
      # the config and matches CI (`dtolnay/rust-toolchain@nightly` +
      # `cargo +nightly fmt`), so `cargo fmt` / `just fmt` in this shell are
      # correct by default and can't reintroduce that churn.
      toolchain =
        with fenix.packages.${system};
        combine [
          stable.cargo
          stable.rustc
          stable.rust-std
          stable.clippy
          latest.rustfmt
        ];
      craneLib = (crane.mkLib pkgs).overrideToolchain toolchain;
      root = ../.;

      # The ONE source of truth for "which macOS does this build require". rustc
      # stamps LC_BUILD_VERSION from MACOSX_DEPLOYMENT_TARGET, and in the crane
      # build nixpkgs' stdenv sets that from the platform default — currently
      # "14.0" (lib/systems/default.nix). Reading it back here, instead of
      # hardcoding a number, keeps three things that MUST agree in lockstep: the
      # packaged binary's minos, the bundle plist's LSMinimumSystemVersion, and
      # the devShell's MACOSX_DEPLOYMENT_TARGET (which rustc would otherwise
      # default to 11.0, since no cc-wrapper flag reaches it). A nixpkgs bump that
      # moves the platform default now moves all three at once.
      minOsVersion = pkgs.stdenv.hostPlatform.darwinMinVersion;

      args = {
        src = lib.fileset.toSource {
          inherit root;
          fileset = lib.fileset.unions [
            (craneLib.fileset.commonCargoSources root)
            (lib.fileset.fileFilter (file: file.hasExt "plist") root)
          ];
        };
        strictDeps = true;
        doCheck = false;

        # Lives in the SHARED args, not just the `buildPackage` call: crane keys
        # `cargoArtifacts` reuse on the argument set, so if `buildDepsOnly` and
        # `buildPackage` disagree on cargoExtraArgs every dependency silently
        # rebuilds. Scoping matters once the workspace lands (root + crates/rift-
        # client + crates/rift-protocol): `--package rift-wm --bins` builds only the
        # root package's two binaries (rift, rift-cli) and keeps the rift-client
        # examples' ctrlc/nix/dispatch2 dev-dependency subtree out of the build.
        cargoExtraArgs = "--locked --package rift-wm --bins";

        nativeBuildInputs = [ ];
        # Mission Control previews go through ScreenCaptureKit, which only exists in
        # the 12.3+ SDK; pin it explicitly instead of inheriting whatever stdenv's
        # default apple-sdk happens to be (11.x historically) so a nixpkgs bump
        # can't silently break the objc2 framework link.
        #
        # The pin is apple-sdk_14, NOT _15, because 14.4 is also what the currently
        # pinned nixpkgs' stdenv defaults to: the clang/bintools wrappers already
        # reference that exact store path, so pinning it costs ZERO extra download
        # while pinning _15 added a second SDK (31 MiB fetch, 459 MiB unpacked) to
        # every build and every dev shell. 14.4 covers every SCK symbol this tree
        # uses (SCShareableContent, SCContentFilter, SCStreamConfiguration,
        # SCScreenshotManager, SCCaptureResolutionType — all <= 14.0). If a future
        # nixpkgs moves stdenv's default off 14.x, bump this pin to match the new
        # default rather than leaving two SDKs in the closure.
        #
        # NO darwinMinVersionHook. It used to be `(pkgs.darwinMinVersionHook "12.3")`
        # here, on the theory that the weak-linked SCK symbols need a 12.3 floor.
        # The hook only RAISES MACOSX_DEPLOYMENT_TARGET, and nixpkgs hands
        # aarch64-darwin 14.0 by default (lib/systems/default.nix), so it never
        # fired: the binary this flake ships has always been stamped
        # `LC_BUILD_VERSION minos 14.0`, verified with `otool -l`. It was a no-op
        # asserting a floor the build does not honour, so it is gone and 14.0 is
        # documented as the real requirement instead (see minOsVersion below and
        # LSMinimumSystemVersion in the bundle plist). If a future nixpkgs lowers
        # its default below what ScreenCaptureKit needs, reintroduce the hook —
        # but then also fix the plist, because the two must agree.
        #
        # No explicit `-framework` flags: the objc2-* crates emit
        # `#[link(kind = "framework")]` themselves, so SDK availability is
        # sufficient. The SDK is a target dependency, i.e. `buildInputs`, which
        # keeps `strictDeps = true` valid.
        buildInputs = [ pkgs.apple-sdk_14 ];
      };

      build = craneLib.buildPackage (
        args
        // {
          cargoArtifacts = craneLib.buildDepsOnly args;
        }
      );

      # Wrap built binaries in proper macOS app bundle for TCC permissions
      rift-app = pkgs.clangStdenv.mkDerivation {
        pname = "rift";
        version = "0.1.0";
        
        buildInputs = [ build ];
        
        dontUnpack = true;
        
        # codesign needs to touch the Mach-O binary outside the sandbox
        __noChroot = true;
        
        installPhase = ''
          # Create proper macOS app bundle structure for accessibility permissions
          mkdir -p $out/Applications/Rift.app/Contents/MacOS
          mkdir -p $out/Applications/Rift.app/Contents/Resources
          
          # Install binaries from crane build into app bundle
          cp ${build}/bin/rift $out/Applications/Rift.app/Contents/MacOS/
          chmod +x $out/Applications/Rift.app/Contents/MacOS/rift
          
          # Create Info.plist for proper app identification
          cat > $out/Applications/Rift.app/Contents/Info.plist << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>rift</string>
  <key>CFBundleIdentifier</key>
  <string>git.acsandmann.rift</string>
  <key>CFBundleName</key>
  <string>Rift</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>LSMinimumSystemVersion</key>
  <string>${minOsVersion}</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSScreenCaptureUsageDescription</key>
  <string>Rift captures window thumbnails to render Mission Control previews.</string>
</dict>
</plist>
EOF
          
          # In-sandbox ad-hoc codesign (placeholder). The nix sandbox has no keychain
          # access, so a stable identity can't be applied here — `just install` and the
          # darwin module's activation script RE-SIGN with the stable `rift-codesign`
          # identity (see nix/module.nix / justfile) so TCC/Accessibility persists.
          # --identifier binds CFBundleIdentifier=git.acsandmann.rift into the signature.
          /usr/bin/codesign --force --sign - \
            --identifier git.acsandmann.rift \
            $out/Applications/Rift.app/Contents/MacOS/rift
          /usr/bin/codesign --force --sign - \
            --identifier git.acsandmann.rift \
            $out/Applications/Rift.app
          
          # Also create symlinks in bin/ for CLI access
          mkdir -p $out/bin
          ln -s $out/Applications/Rift.app/Contents/MacOS/rift $out/bin/rift
          
          # Install rift-cli if it was built
          if [ -f ${build}/bin/rift-cli ]; then
            cp ${build}/bin/rift-cli $out/bin/rift-cli
            chmod +x $out/bin/rift-cli
          fi
        '';
        
        meta = {
          description = "Rift - A lightweight tiling window manager for macOS";
          homepage = "https://github.com/acsandmann/rift";
          platforms = lib.platforms.darwin;
          mainProgram = "rift";
        };
      };

    in
    {
      # Build outputs
      checks.rift = build;  # Ensure build succeeds in CI

      packages.rift = rift-app;  # Main package with app bundle
      packages.rift-unwrapped = build;  # Raw binaries for development
      packages.default = rift-app;

      devshells.default = {
        packages = [
          toolchain
          # Same SDK the crane build pins (ScreenCaptureKit); see `args`. This is
          # also stdenv's default SDK, so it is already in the shell's closure via
          # the clang wrapper — the entry only makes the dependency explicit.
          pkgs.apple-sdk_14
        ];
        # nixpkgs' apple-sdk doesn't ship libiconv, and unlike the crane build
        # (clangStdenv injects it via NIX_LDFLAGS) the devshell has to add it
        # itself — otherwise ambient `cargo build`/`test` fail to LINK with
        # `ld: library 'iconv' not found`. Prepend it to the SDK's LIBRARY_PATH.
        env = [
          {
            name = "LIBRARY_PATH";
            prefix = "${pkgs.libiconv}/lib";
          }
          # devshell builds on a NAKED stdenv, so no nixpkgs setup hook ever runs
          # here — neither apple-sdk's DEVELOPER_DIR hook nor any deployment-target
          # default. The cc/bintools wrappers read DEVELOPER_DIR at runtime (falling
          # back to the stdenv default SDK when it is unset) and derive SDKROOT from
          # it, so point it at the same SDK the crane build pins or an ambient
          # `cargo build` can't find ScreenCaptureKit.
          {
            name = "DEVELOPER_DIR";
            value = "${pkgs.apple-sdk_14}";
          }
          # MACOSX_DEPLOYMENT_TARGET is set to exactly what the crane build gets
          # from stdenv, so a devShell `cargo build` and `nix build` stamp the SAME
          # LC_BUILD_VERSION. It is NOT redundant: rustc does not read the cc
          # wrapper's -mmacosx-version-min, it stamps its own built-in default for
          # aarch64-apple-darwin (11.0 — `rustc --print deployment-target` proves
          # it) whenever this variable is unset, so dev-local binaries used to
          # claim macOS 11 while the packaged one claimed 14. Setting it to the
          # platform default rather than a lower floor also keeps the linker quiet:
          # a lower target against nixpkgs' 14.0 dylibs earns "built for newer
          # macOS version than being linked" on every build.
          {
            name = "MACOSX_DEPLOYMENT_TARGET";
            value = minOsVersion;
          }
        ];
        commands = [
          {
            help = "";
            name = "hot";
            command = "${pkgs.watchexec}/bin/watchexec -e rs -w src -w Cargo.toml -w Cargo.lock -r ${toolchain}/bin/cargo run -- $@";
          }
        ];
      };
    };
}
