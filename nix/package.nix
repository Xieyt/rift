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
        # default apple-sdk happens to be (11.x historically, 14.4 in the currently
        # pinned nixpkgs) so a nixpkgs bump can't silently break the objc2
        # framework link. darwinMinVersionHook is a FLOOR: it raises
        # MACOSX_DEPLOYMENT_TARGET to 12.3 if it is lower (a no-op while stdenv
        # already defaults to 14.0) so the weak-linked SCK symbols resolve instead
        # of aborting at load. No explicit `-framework` flags: the objc2-* crates
        # emit `#[link(kind = "framework")]` themselves, so SDK availability is
        # sufficient. Both are target dependencies, i.e. `buildInputs`, which keeps
        # `strictDeps = true` valid.
        buildInputs = [
          pkgs.apple-sdk_15
          (pkgs.darwinMinVersionHook "12.3")
        ];
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
          # Same 12.3+ SDK the crane build pins (ScreenCaptureKit); see `args`.
          pkgs.apple-sdk_15
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
          # here — neither apple-sdk's DEVELOPER_DIR hook nor darwinMinVersionHook.
          # The cc/bintools wrappers read DEVELOPER_DIR at runtime (falling back to
          # the stdenv default SDK when it is unset) and derive SDKROOT from it, so
          # point it at the same SDK the crane build pins or an ambient
          # `cargo build` can't find ScreenCaptureKit. No MACOSX_DEPLOYMENT_TARGET
          # here on purpose: the wrappers' baked-in default (14.0) already clears
          # the 12.3 floor darwinMinVersionHook enforces for the crane build, and
          # hard-setting a LOWER target would only earn "built for newer macOS
          # version than being linked" warnings against nixpkgs' own dylibs.
          {
            name = "DEVELOPER_DIR";
            value = "${pkgs.apple-sdk_15}";
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
