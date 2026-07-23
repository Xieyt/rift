# rift fork — build, test, and live-swap recipes.
#
# ENVIRONMENT: recipes need the Nix dev env (fenix toolchain + matched macOS SDK
# + libiconv). Enter it automatically with direnv (`.envrc` runs `use flake`;
# run `direnv allow` once) or manually with `nix develop`. Without it, anything
# that LINKS fails with `ld: library not found for -liconv` (wrong ambient SDK).
#   - `check` / `clippy` / `fmt` don't link — fine even in a plain shell.
#   - `test` links, so its recipe goes through `nix develop` to be safe.
#   - `nix build` is the real end-to-end build/bundle.
# See FORK.md for the full story.

uid := `id -u`
agent := "gui/" + uid + "/git.acsandmann.rift"
app := "/Applications/Rift.app"
signing_cert := "rift-codesign"
sys_keychain := "/Library/Keychains/System.keychain"

# list recipes
default:
    @just --list

# ---------------------------------------------------------------------------
# fast, ambient (no linking) — safe to run anytime, e.g. during a rebase
# ---------------------------------------------------------------------------

# type/borrow check (does NOT link — always works in the plain shell)
check:
    cargo check

# lint
clippy:
    cargo clippy --all-targets

# apply / verify formatting
fmt:
    cargo fmt
fmt-check:
    cargo fmt --check

# ---------------------------------------------------------------------------
# tests — run through `nix develop` so they link (see ENVIRONMENT above).
# Excludes the SkyLight/window-server tests (need a real GUI session) and one
# reactor test that fails on upstream too.
# ---------------------------------------------------------------------------

# fast deterministic logic tests (layout, raise, reactor, config, stack-line) — no GUI
test:
    nix develop -c cargo test --lib -- layout_engine actor::raise_manager actor::reactor::tests common::config ui::stack_line --skip topology_change_clears_stale_pending_hide_target

# whole library suite — only meaningful in a real GUI session
test-all:
    nix develop -c cargo test --lib

# ---------------------------------------------------------------------------
# real builds — through Nix, so the linker/SDK are correct
# ---------------------------------------------------------------------------

# full macOS .app bundle (codesigned git.acsandmann.rift) -> ./result
build:
    nix build .#rift

# raw binaries only (fast: no bundle/codesign) -> ./result/bin/{rift,rift-cli}
build-raw:
    nix build .#rift-unwrapped

# CI-equivalent: crane build as a flake check
flake-check:
    nix flake check

# ---------------------------------------------------------------------------
# run a nix-built binary to test (proves the merged code links AND runs)
# ---------------------------------------------------------------------------

# smoke test: build raw binaries, then confirm both actually launch.
# `--help` is safe: clap parses and exits before any window-manager logic runs.
smoke: build-raw
    ./result/bin/rift --help >/dev/null
    ./result/bin/rift-cli --help >/dev/null
    @echo "OK: nix-built rift + rift-cli link and run"

# run the built CLI against the *currently running* WM
# e.g. `just cli execute window focus east --activate`
cli *ARGS: build-raw
    ./result/bin/rift-cli {{ARGS}}

# enter the nix dev shell (fenix toolchain)
dev:
    nix develop

# ---------------------------------------------------------------------------
# live swap — replace the running WM on THIS mac to test now
# ---------------------------------------------------------------------------
#
# This is the quick, local path: build the app bundle, drop it over
# /Applications/Rift.app (rsync -a preserves the ad-hoc codesign so TCC keeps
# recognizing `git.acsandmann.rift`), then restart the launchd agent.
#
# The "proper" path is `darwin-rebuild switch` with your system flake pointing
# at this checkout — use that for a permanent install. `just install` is for
# fast iterate-and-test loops.

# build + install over /Applications/Rift.app + restart the agent
install: build
    #!/usr/bin/env bash
    set -euo pipefail
    src="$(pwd)/result/Applications/Rift.app"
    # Prefer a stable self-signed identity so macOS keeps the Accessibility grant
    # across rebuilds (TCC keys on the signing identity, not the per-build cdhash).
    # Falls back to ad-hoc (re-grant each build) if the cert isn't set up.
    if security find-certificate -c "{{signing_cert}}" "{{sys_keychain}}" >/dev/null 2>&1; then
      sign="{{signing_cert}}"
    else
      sign="-"
      echo "!! '{{signing_cert}}' not in System keychain; signing ad-hoc."
      echo "   Run \`just setup-signing-cert\` once so Accessibility survives rebuilds."
    fi
    echo "==> replacing {{app}} with fresh build (sudo required)"
    sudo rm -rf "{{app}}"
    sudo rsync -a "$src/" "{{app}}/"
    sudo /usr/bin/codesign --force --sign "$sign" --identifier git.acsandmann.rift "{{app}}/Contents/MacOS/rift"
    sudo /usr/bin/codesign --force --sign "$sign" --identifier git.acsandmann.rift "{{app}}"
    id=$(codesign -dv "{{app}}/Contents/MacOS/rift" 2>&1 | sed -n 's/^Identifier=//p')
    echo "==> installed; identifier=$id signed-with=$sign"
    just restart
    echo "✓ rift replaced and restarted."
    if [ "$sign" = "-" ]; then
      echo "  (ad-hoc) if focus/keys break, re-grant Rift under System Settings →"
      echo "  Privacy & Security → Accessibility, then \`just restart\`."
    fi

# ---------------------------------------------------------------------------
# fast dev loop — skip the hermetic `nix build` entirely
# ---------------------------------------------------------------------------
#
# `nix build` recompiles the WHOLE rift-wm crate every time (the sandbox can't
# see your working target/, so there's no incremental reuse) and then rebuilds
# the .app bundle. For iterate-and-test that's the slow path.
#
# `dev-install` instead builds with plain `cargo` inside the nix dev shell
# (reuses target/ incrementally — seconds after the first build), then swaps
# ONLY the Mach-O into the existing /Applications/Rift.app and re-signs. This is
# safe because TCC keys on the signing IDENTITY + --identifier, not the cdhash,
# so the Accessibility grant survives an in-place binary swap.
#
# Requires a prior full `just install` to create the bundle. Default profile is
# `release-fast` (opt-level 2, incremental); pass `dev` for the fastest compile.
dev-install profile="release-fast" config="dev-config.toml":
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -d "{{app}}" ]; then
      echo "!! {{app}} missing — run \`just install\` once to create the bundle." >&2
      exit 1
    fi
    # Snapshot the running layout BEFORE rebuilding, using the current rift-cli
    # (matches the running rift), so the post-restart `--restore` reloads it.
    # `FRESH=1` (or `just dev-install-fresh`) skips the snapshot and clears the
    # master file for a clean slate (e.g. to escape a scrambled layout).
    if [ "${FRESH:-0}" = "1" ]; then
      rm -f "$HOME/.rift/layout.ron"
      echo "==> fresh: cleared ~/.rift/layout.ron (clean layout on restart)"
    elif [ -x "target/{{profile}}/rift-cli" ]; then
      target/{{profile}}/rift-cli execute save-layout --master 2>/dev/null \
        && echo "==> saved layout snapshot (~/.rift/layout.ron)" \
        || echo "note: no running rift to snapshot (fresh start?)"
    fi
    echo "==> cargo build (--profile {{profile}}, incremental)"
    nix develop -c cargo build --profile {{profile}} --bin rift --bin rift-cli
    bin="target/{{profile}}/rift"
    if security find-certificate -c "{{signing_cert}}" "{{sys_keychain}}" >/dev/null 2>&1; then
      sign="{{signing_cert}}"
    else
      sign="-"
      echo "!! '{{signing_cert}}' not in System keychain; signing ad-hoc (re-grant Accessibility)."
    fi
    echo "==> swapping {{app}}/Contents/MacOS/rift (sudo required)"
    # atomic rename: the running process keeps its old inode; no mmap corruption.
    tmp="{{app}}/Contents/MacOS/.rift.new"
    sudo cp -f "$bin" "$tmp"
    sudo mv -f "$tmp" "{{app}}/Contents/MacOS/rift"
    sudo /usr/bin/codesign --force --sign "$sign" --identifier git.acsandmann.rift "{{app}}/Contents/MacOS/rift"
    sudo /usr/bin/codesign --force --sign "$sign" --identifier git.acsandmann.rift "{{app}}"
    # Ensure the launch agent runs [--restore --config <cfg>] and has
    # /opt/homebrew/bin on PATH so run_on_start scripts resolve `sketchybar`
    # (temporary shim until `just fern` bakes PATH into the nix module). Any
    # plist change needs a bootout/bootstrap — kickstart won't pick up edits.
    plist="$HOME/Library/LaunchAgents/git.acsandmann.rift.plist"
    hb="/opt/homebrew/bin"
    cfg=""
    if [ -n "{{config}}" ] && [ -f "{{config}}" ]; then
      cfg="$(cd "$(dirname "{{config}}")" && pwd)/$(basename "{{config}}")"
    fi
    if [ -n "$cfg" ] && [ -f "$plist" ]; then
      chmod u+w "$plist" 2>/dev/null || true
      need_reload=0
      cur_restore="$(/usr/libexec/PlistBuddy -c 'Print :ProgramArguments:1' "$plist" 2>/dev/null || echo '')"
      cur_cfg="$(/usr/libexec/PlistBuddy -c 'Print :ProgramArguments:3' "$plist" 2>/dev/null || echo '')"
      if [ "$cur_restore" != "--restore" ] || [ "$cur_cfg" != "$cfg" ]; then
        /usr/libexec/PlistBuddy -c 'Delete :ProgramArguments' "$plist" 2>/dev/null || true
        /usr/libexec/PlistBuddy -c 'Add :ProgramArguments array' "$plist"
        /usr/libexec/PlistBuddy -c "Add :ProgramArguments:0 string {{app}}/Contents/MacOS/rift" "$plist"
        /usr/libexec/PlistBuddy -c 'Add :ProgramArguments:1 string --restore' "$plist"
        /usr/libexec/PlistBuddy -c 'Add :ProgramArguments:2 string --config' "$plist"
        /usr/libexec/PlistBuddy -c "Add :ProgramArguments:3 string $cfg" "$plist"
        echo "==> set launch agent args [--restore --config $cfg]"
        need_reload=1
      fi
      cur_path="$(/usr/libexec/PlistBuddy -c 'Print :EnvironmentVariables:PATH' "$plist" 2>/dev/null || echo '')"
      case ":$cur_path:" in
        *":$hb:"*) : ;;
        *)
          new_path="$hb${cur_path:+:$cur_path}"
          /usr/libexec/PlistBuddy -c "Set :EnvironmentVariables:PATH $new_path" "$plist" 2>/dev/null \
            || { /usr/libexec/PlistBuddy -c 'Add :EnvironmentVariables dict' "$plist" 2>/dev/null || true; \
                 /usr/libexec/PlistBuddy -c "Add :EnvironmentVariables:PATH string $new_path" "$plist"; }
          echo "==> added $hb to agent PATH"
          need_reload=1
          ;;
      esac
      if [ "$need_reload" = 1 ]; then
        # bootout is async; bootstrapping before teardown finishes returns EIO(5).
        launchctl bootout {{agent}} 2>/dev/null || true
        for _ in $(seq 1 50); do
          launchctl print {{agent}} >/dev/null 2>&1 || break
          sleep 0.1
        done
        launchctl bootstrap gui/{{uid}} "$plist" 2>/dev/null \
          || { sleep 0.5; launchctl bootstrap gui/{{uid}} "$plist"; }
        echo "✓ rift swapped ({{profile}}), config=$cfg, agent reloaded."
      else
        just restart
        echo "✓ rift swapped ({{profile}}), config=$cfg, layout restored, restarted."
      fi
    else
      [ -n "{{config}}" ] && [ -z "$cfg" ] && echo "note: config '{{config}}' not found; using the agent's existing --config."
      just restart
      echo "✓ rift swapped ({{profile}}) and restarted."
    fi

    # Reload hyperkey launch agent (workaround for it not restarting cleanly)
    launchctl unload ~/Library/LaunchAgents/com.user.hyperkey-restart.plist 2>/dev/null || true
    launchctl load ~/Library/LaunchAgents/com.user.hyperkey-restart.plist 2>/dev/null || true

# Like `dev-install` but discards the saved layout snapshot for a clean slate.
dev-install-fresh profile="release-fast" config="dev-config.toml":
    @FRESH=1 just dev-install {{profile}} {{config}}

# one-time: create a stable self-signed code-signing cert in the System keychain
# so the Accessibility grant survives rebuilds. Requires admin.
setup-signing-cert:
    #!/usr/bin/env bash
    set -euo pipefail
    cert="{{signing_cert}}"; kc="{{sys_keychain}}"
    if security find-certificate -c "$cert" "$kc" >/dev/null 2>&1; then
      echo "✓ '$cert' already present in System keychain"; exit 0
    fi
    tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
    # Pin system LibreSSL — Homebrew's OpenSSL 3.x writes a p12 macOS can't import.
    ossl=/usr/bin/openssl
    printf '[req]\ndistinguished_name = dn\nx509_extensions = v3\nprompt = no\n[dn]\nCN = %s\n[v3]\nbasicConstraints = critical,CA:false\nkeyUsage = critical,digitalSignature\nextendedKeyUsage = critical,codeSigning\n' "$cert" > "$tmp/cert.cnf"
    "$ossl" req -x509 -newkey rsa:2048 -nodes -days 7300 \
      -keyout "$tmp/key.pem" -out "$tmp/cert.pem" -config "$tmp/cert.cnf" -extensions v3
    # Legacy PBE + SHA1 MAC + non-empty password => importable by macOS `security`.
    "$ossl" pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
      -out "$tmp/cert.p12" -passout pass:rift -name "$cert" \
      -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1
    echo "==> importing '$cert' into System keychain (sudo required)"
    sudo security import "$tmp/cert.p12" -k "$kc" -P rift -A -T /usr/bin/codesign
    sudo security add-trusted-cert -d -r trustRoot -p codeSign -k "$kc" "$tmp/cert.pem"
    echo "✓ '$cert' created and trusted for code signing."
    echo "  Run \`just install\` next; the app signs with this stable identity and"
    echo "  the Accessibility grant persists across future rebuilds."

# restart the running WM (picks up a freshly-installed binary)
restart:
    launchctl kickstart -k {{agent}}

# stop / start the WM agent
stop:
    launchctl bootout {{agent}} || true
start:
    launchctl bootstrap gui/{{uid}} ~/Library/LaunchAgents/git.acsandmann.rift.plist || launchctl kickstart {{agent}}

# is it running, and which binary?
status:
    @launchctl print {{agent}} 2>/dev/null | rg -i "state|program|path" || echo "rift agent not loaded"

# follow the logs
logs:
    tail -f /tmp/rift.err.log /tmp/rift.out.log

# probe REAL on-screen window geometry from the window server (ground truth).
# Detects tile overlaps; with --rift, diffs Rift's intended frames vs reality.
# e.g. `just probe`  |  `just probe --app Emacs alacritty`  |  `just probe --rift`
probe *ARGS:
    uv run scripts/wm-probe.py {{ARGS}}

# ---------------------------------------------------------------------------
# fork maintenance (see FORK.md)
# ---------------------------------------------------------------------------

# how far from upstream + what we carry on top
upstream-status:
    git fetch upstream
    @echo "behind / ahead vs upstream/main:"
    @git rev-list --left-right --count upstream/main...HEAD
    @git log --oneline upstream/main..HEAD

# rebase our branch onto latest upstream (then resolve, `just check`, force-push)
sync:
    git fetch upstream
    git push origin upstream/main:main
    git rebase upstream/main
