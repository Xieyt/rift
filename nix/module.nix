{ self, ... }:

{
  flake.darwinModules.default =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      cfg = config.services.rift;

      toml = pkgs.formats.toml { };

      logRotateScript = pkgs.writeShellScript "rift-logrotate" ''
        mkdir -p "${cfg.logDir}"
        find "${cfg.logDir}" -name "*.log" -mmin +1440 -delete
      '';

      configFile =
        if cfg.config == null then
          null
        else if lib.isPath cfg.config || lib.isString cfg.config then
          cfg.config
        else
          toml.generate "rift.toml" cfg.config;
    in
    {
      options.services.rift = {
        enable = lib.mkEnableOption "Enable rift window manager service";

        package = lib.mkOption {
          type = lib.types.package;
          default = self.packages.${pkgs.system}.default;
          description = "rift (not rift-cli) package to use";
        };

        config = lib.mkOption {
          type = with lib.types; nullOr (oneOf [
            str
            path
            toml.type
          ]);
          description = "Configuration settings for rift. Also accepts paths (string or path type) to a config file. If null, rift uses internal defaults.";
          default = null;
        };

        logLevel = lib.mkOption {
          type = lib.types.str;
          default = "error,warn,info,rift_wm::actor::reactor=debug,rift_wm::layout_engine=debug,rift_wm::actor::raise_manager=debug";
          description = "RUST_LOG value for rift. Supports per-module log levels.";
        };

        logDir = lib.mkOption {
          type = lib.types.str;
          description = "Directory for rift log files. Must be an absolute path.";
        };

        signingIdentity = lib.mkOption {
          type = lib.types.str;
          default = "rift-codesign";
          description = ''
            Code-signing identity (keychain cert name) used to sign Rift.app during
            activation. A stable self-signed identity keeps the macOS Accessibility
            grant across rebuilds (TCC keys on the signing identity, not the
            per-build cdhash). With `manageSigningIdentity = true` (default) this
            self-signed identity is created automatically on first activation.
            Set to "-" to sign ad-hoc (Accessibility re-granted each rebuild).
          '';
        };

        manageSigningIdentity = lib.mkOption {
          type = lib.types.bool;
          default = true;
          description = ''
            If true (default), activation auto-creates the `signingIdentity`
            self-signed code-signing cert in the System keychain (idempotent:
            created once, reused forever) and trusts it for code signing. This lets
            a plain `darwin-rebuild switch` keep the Accessibility grant across
            rebuilds with no manual keychain setup or repo clone. Set false to
            manage the identity yourself or to keep ad-hoc signing.
          '';
        };
      };

      config = lib.mkIf cfg.enable {
        # Install rift-cli to systemPackages for CLI access
        environment.systemPackages = [ cfg.package ];

        # Custom app installation that preserves codesign identity
        # We bypass nix-darwin's automatic /Applications/Nix Apps/ copying (which re-signs)
        # and manage the installation ourselves with correct TCC identity preserved.
        system.activationScripts.applications.text = lib.mkAfter ''
          app_source="${cfg.package}/Applications/Rift.app"
          app_target="/Applications/Rift.app"
          
          echo "Installing Rift.app with preserved TCC identity..." >&2
          
          # Remove old installation if exists
          if [ -e "$app_target" ]; then
            rm -rf "$app_target"
          fi
          
          # Copy app bundle (rsync preserves codesign)
          ${pkgs.rsync}/bin/rsync -a "$app_source/" "$app_target/"
          
          # Ensure a stable code-signing identity exists so TCC (Accessibility)
          # survives rebuilds, then sign with it. Uses system LibreSSL (/usr/bin)
          # because Homebrew/nix OpenSSL 3.x writes a p12 macOS can't import.
          sign="${cfg.signingIdentity}"
          keychain=/Library/Keychains/System.keychain
          ${lib.optionalString cfg.manageSigningIdentity ''
            if [ "$sign" != "-" ] && ! /usr/bin/security find-certificate -c "$sign" "$keychain" >/dev/null 2>&1; then
              echo "rift: creating stable code-signing identity '$sign'..." >&2
              cdir=$(mktemp -d)
              printf '[req]\ndistinguished_name=dn\nx509_extensions=v3\nprompt=no\n[dn]\nCN=%s\n[v3]\nbasicConstraints=critical,CA:false\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=critical,codeSigning\n' "$sign" > "$cdir/c.cnf"
              if /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 7300 \
                   -keyout "$cdir/k.pem" -out "$cdir/c.pem" -config "$cdir/c.cnf" -extensions v3 >/dev/null 2>&1 \
                 && /usr/bin/openssl pkcs12 -export -inkey "$cdir/k.pem" -in "$cdir/c.pem" \
                   -out "$cdir/c.p12" -passout pass:rift -name "$sign" \
                   -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 >/dev/null 2>&1 \
                 && /usr/bin/security import "$cdir/c.p12" -k "$keychain" -P rift -A -T /usr/bin/codesign >/dev/null 2>&1; then
                /usr/bin/security add-trusted-cert -d -r trustRoot -p codeSign -k "$keychain" "$cdir/c.pem" >/dev/null 2>&1 || true
                echo "rift: code-signing identity '$sign' created." >&2
              else
                echo "rift: WARNING could not create '$sign'; will sign ad-hoc." >&2
              fi
              rm -rf "$cdir"
            fi
          ''}
          if [ "$sign" != "-" ] && ! /usr/bin/security find-certificate -c "$sign" "$keychain" >/dev/null 2>&1; then
            echo "rift: signing identity '$sign' unavailable; signing ad-hoc (re-grant Accessibility each rebuild)." >&2
            sign="-"
          fi
          /usr/bin/codesign --force --sign "$sign" --identifier git.acsandmann.rift "$app_target/Contents/MacOS/rift"
          /usr/bin/codesign --force --sign "$sign" --identifier git.acsandmann.rift "$app_target"
          got=$(/usr/bin/codesign -dv "$app_target/Contents/MacOS/rift" 2>&1 | grep "Identifier=" | cut -d= -f2)
          echo "✓ Rift.app signed: identifier=$got identity=$sign" >&2
        '';

        launchd.user.agents.rift = {
          serviceConfig = {
            Label = "git.acsandmann.rift";

            # Use ProgramArguments (direct exec array) instead of nix-darwin's `command` field.
            # `command` wraps the value in `/bin/sh -c "..."`, which:
            #   1. Breaks on the space in "/Applications/Nix Apps/" (shell splits it)
            #   2. Makes macOS TCC see /bin/sh as the process instead of Rift, defeating
            #      accessibility permissions even after the user grants them.
            # ProgramArguments passes the path as an unquoted array element — no shell
            # interpretation, no space splitting, and launchd exec()s Rift directly so
            # TCC sees the correct bundle identity.
            ProgramArguments =
              [ "/Applications/Rift.app/Contents/MacOS/rift" ]
              ++ lib.optionals (configFile != null) [ "--config" (toString configFile) ];
            EnvironmentVariables = {
              RUST_LOG = cfg.logLevel;
              # todo improve
              PATH = "/run/current-system/sw/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";
            };
            RunAtLoad = true;
            KeepAlive = {
              SuccessfulExit = false;
              Crashed = true;
            };
            StandardOutPath = "${cfg.logDir}/rift.out.log";
            StandardErrorPath = "${cfg.logDir}/rift.err.log";
            ProcessType = "Interactive";
            LimitLoadToSessionType = "Aqua";
            Nice = -20;
          };
        };

        launchd.user.agents.rift-logrotate = {
          serviceConfig = {
            Label = "git.acsandmann.rift.logrotate";
            ProgramArguments = [ "${logRotateScript}" ];
            StartInterval = 3600;
            RunAtLoad = true;
            StandardOutPath = "/tmp/rift-logrotate.log";
            StandardErrorPath = "/tmp/rift-logrotate.log";
          };
        };
      };
    };
}
