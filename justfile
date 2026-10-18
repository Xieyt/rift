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
# A STABLE local code-signing identity is what lets the Accessibility and
# Screen Recording grants survive a rebuild: TCC keys on the identity, not
# on the per-build cdhash, so an ad-hoc signature re-prompts every time.
#
# Resolved in three steps, first hit wins:
#   1. `just signing_cert=my-identity install`
#   2. ~/.config/rift/signing-identity, written by the nix module from
#      `services.rift.signingIdentity`. That is how a machine sharing one
#      identity across several locally built apps names it once, instead of
#      hardcoding a machine-specific name in this file.
#   3. rift-codesign, created by `just setup-signing-cert`.
signing_cert := `cat "${XDG_CONFIG_HOME:-$HOME/.config}/rift/signing-identity" 2>/dev/null || echo rift-codesign`
sys_keychain := "/Library/Keychains/System.keychain"
plist := env_var('HOME') + "/Library/LaunchAgents/git.acsandmann.rift.plist"
local_config := justfile_directory() + "/local-config.toml"

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
    nix develop -c cargo clippy --all-targets

# apply / verify formatting.
# NOTE: these go through `nix develop` ON PURPOSE. rustfmt.toml enables unstable
# options, so they only apply under the dev-shell rustfmt, which is pinned to
# NIGHTLY (see nix/package.nix). A plain-shell *stable* `cargo fmt` parses
# rustfmt.toml, warns, silently ignores every unstable option, and reflows the
# entire tree to stable defaults — a massive spurious cross-file diff. These
# recipes used to be bare `cargo fmt`, which meant `just fmt` did exactly that
# whenever the caller was not already inside the dev shell (it is how ~40
# unrelated files once got swept into a feature commit; see FORK.md §6).
# `--all` matches CI (`cargo +nightly fmt --all --check`).
fmt:
    nix develop -c cargo fmt --all
fmt-check:
    nix develop -c cargo fmt --all --check

# ---------------------------------------------------------------------------
# tests — run through `nix develop` so they link (see ENVIRONMENT above).
# Excludes the SkyLight/window-server tests (need a real GUI session) and one
# reactor test that fails on upstream too.
# ---------------------------------------------------------------------------

# Fast deterministic logic tests — everything that runs without a GUI.
#
# This list was curated by *assumption* until 2026-08-11, and the assumption was
# expensive: 13 modules were sitting in the "needs a GUI" bucket while passing
# perfectly well headless, so 148 real tests never ran in the fast suite (`model`
# alone is 70, `actor::spaces` 42). Measured module by module: only `sys` genuinely
# aborts headless (objc weak-reference error from the SkyLight/window-server tests).
#
# Every new test module MUST be added here, or classified in the
# `GUI_OR_DEFERRED` list in `common::config`'s
# `just_test_allowlist_classifies_every_test_module` — that test fails by name if a
# module is in neither, because a curated allowlist fails *silent*: adding a test
# module without adding its filter is a no-op that looks like coverage.
# `ui::hints_bar` was missing for months while FORK.md §2.E claimed those tests ran.
#
# TWO skips, both upstream failures reproduced on a pristine worktree at the
# current stage checkpoint — not our regressions. UPSTREAM-SYNC.md §9, §12.
#   confirmed_window_move_forwards_membership_without_space_switch   (stage 1)
#   empty_active_space_membership_during_wake_race_...               (stage 3)
#
# `topology_change_clears_stale_pending_hide_target` was skipped here from §9
# until the sixth sync's stage 3, when a full sweep of the filtered suite on a
# pristine `b02bb33` worktree found it GREEN: upstream repaired it somewhere in
# dcaaaa8..b02bb33. Verified by deletion, as always, and deleted.
#
# Method, since one-at-a-time discovery wasted a stage: at each checkpoint, loop
# the whole filtered suite on a pristine worktree, skipping each aborting test in
# turn, until it goes green. That yields the complete red set in one pass and
# proves the skip list has no stale entries left.
#
# There were THREE. The first dropped one skipped
# `ax_invalidation_after_quarantine_release_preserves_live_layout_state`, a test
# `792370e` had already renamed — so the filter had been a silent no-op, which is
# the exact failure mode the paragraph above warns about. Deleted, and its
# successor `current_ax_destruction_after_quarantine_release_removes_window`
# passes. Skip lists rot the same way allowlists do: verify by deletion.
#
# The second was `wsid_rekey_preserves_floating_membership_and_position`
# (`792370e`, bisected in §9.2: a floated window snapped back into the tiling
# across a WindowServerId rekey). Dropped in the fifth sync's stage 1: `e4aa60d`
# rewrote `identify_stale_windows` into the token-gated inventory pipeline, and
# the test now passes both on a pristine `upstream/main` (83f3b66) and here.
# Verified by deletion, as above.
#
# The third is `confirmed_window_move_forwards_membership_without_space_switch`,
# added by the sixth sync's stage 1. It is UPSTREAM'S, verified by deletion: a
# pristine worktree at the stage-1 checkpoint `0ddcf73` aborts on it too (SIGABRT,
# "failed to initiate panic"). The fork has never edited `src/actor/spaces.rs`.
# A sweep of the whole filtered suite on that pristine tree found exactly two red
# tests — this one and the `topology_change_...` above — so the skip list is
# complete for this checkpoint. Re-verify by deletion at the next one.
test:
    nix develop -c cargo test --lib -- layout_engine model common::config actor::raise_manager actor::reactor::tests actor::reactor::mouse_focus_publisher_tests actor::reactor::managers actor::reactor::events actor::reactor::animation actor::reactor::main_window actor::reactor::utils actor::spaces actor::config actor::drag actor::gesture actor::reactor::gesture actor::input actor::menu_bar actor::notification_center actor::process actor::stack_line actor::window_notify actor::hints_bar ui::settings ui::hints_bar ui::menu_bar ui::mission_control --skip confirmed_window_move_forwards_membership_without_space_switch --skip empty_active_space_membership_during_wake_race_does_not_blank_known_active_windows --skip inventory_does_not_replace_geometry_owned_by_pending_rift_transaction

# whole library suite — only meaningful in a real GUI session
test-all:
    nix develop -c cargo test --lib

# Workspace-member crate tests that neither `test` nor `test-all` ever runs:
# both use `cargo test --lib`, which builds only the root package's lib target,
# so `crates/rift-protocol` and `crates/rift-client` never execute their own
# `#[cfg(test)]` suites (e.g. `crates/rift-protocol/src/transport.rs`'s wire-
# format tests, `crates/rift-client/src/lib.rs`'s Mach message framing tests).
# Guarded against silent coverage loss the same way `test:`'s filters are:
# `common::config`'s `just_test_allowlist_classifies_every_test_module` walks
# `crates/*/src` too and classifies via these `-p` package names.
test-crates:
    nix develop -c cargo test -p rift-protocol -p rift-client --lib

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
      origin="repo default"
      [ -f "${XDG_CONFIG_HOME:-$HOME/.config}/rift/signing-identity" ] && origin="~/.config/rift/signing-identity"
      echo "signing with: $sign  ($origin)"
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
#
# Nothing else to restart. This used to reload the hyperkey agent too, because
# both apps put an event tap at the same stage and the most recently created
# one sat in front — so rift restarting pushed hyperkey's modifier injection
# behind it and every trigger-key binding stopped matching. That is gone:
# Left Command is swapped to F20 by the HID driver, below every tap, so rift
# sees it no matter who started when. Reloading that agent now does nothing
# but relaunch a retired app, which overwrites the whole hidutil swap list.
restart:
    launchctl kickstart -k {{agent}}

# stop / start the WM agent
stop:
    launchctl bootout {{agent}} || true
start:
    launchctl bootstrap gui/{{uid}} ~/Library/LaunchAgents/git.acsandmann.rift.plist || launchctl kickstart {{agent}}

# is it running, and which binary?
status:
    #!/usr/bin/env bash
    set -uo pipefail
    launchctl print {{agent}} 2>/dev/null | grep -iE "state|program|path" || echo "rift agent not loaded"
    cfg=$(/usr/libexec/PlistBuddy -c "Print :ProgramArguments:2" "{{plist}}" 2>/dev/null || echo "?")
    if [ "$cfg" = "{{local_config}}" ]; then
      echo "  config   $cfg   (LOCAL — not the system one; \`just use-system-config\` to revert)"
    else
      echo "  config   $cfg"
    fi

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

# UPSTREAM-SYNC.md §7: sync on structural change, not on a calendar. Measured: 13
# flat commits cost 0 hand-resolved hunks; single structural commits cost 6 each.
# A commit count tells you nothing, so this reports the tripwires instead.
# drift vs upstream + structural tripwires (decides whether to sync now)
upstream-status:
    #!/usr/bin/env bash
    set -uo pipefail
    git fetch upstream --quiet
    echo "=== behind / ahead vs upstream/main (left=new upstream, right=ours) ==="
    git rev-list --left-right --count upstream/main...HEAD
    behind=$(git rev-list --count HEAD..upstream/main)
    if [ "$behind" -eq 0 ]; then echo "up to date."; exit 0; fi
    echo "=== $behind new upstream commit(s) ==="
    git log --oneline --no-merges HEAD..upstream/main
    echo
    echo "=== structural tripwires — ANY hit means sync now, not later ==="
    # 2>/dev/null: a tripwire HIT exits 1 by design, and `just` would print its own
    # "recipe failed" line over the report. The report itself is on stdout.
    just upstream-tripwire 2>/dev/null || true
    echo
    echo "next: just sync   (checkpointed merges, NOT a rebase — see UPSTREAM-SYNC.md §3)"

# `upstream-status` wraps this for humans; .github/workflows/upstream-tripwire.yml
# consumes it and opens an issue. One implementation, two callers — UPSTREAM-SYNC.md
# §12.10 item 1: the 2026-10 backlog grew to 132 commits because this rule existed
# only as prose nobody was reminded of.
# structural tripwires only, as markdown; exits 1 when any fires
upstream-tripwire:
    #!/usr/bin/env bash
    set -uo pipefail
    git rev-parse --verify --quiet upstream/main >/dev/null || {
      echo "no upstream/main ref — run: git fetch upstream"; exit 1; }
    behind=$(git rev-list --count HEAD..upstream/main)
    if [ "$behind" -eq 0 ]; then echo "- up to date with \`upstream/main\`."; exit 0; fi
    hit=0
    # `merge-base..upstream/main`, NOT `HEAD..upstream/main`. The two-dot form here
    # is a *tree* diff, so it also reports everything OUR side carries — and we
    # permanently carry `arc-swap` in Cargo.toml plus five command variants in
    # crates/rift-protocol. Read that way the check fired on every run forever,
    # printing our own additions as deletions (2026-10-18, UPSTREAM-SYNC.md §13).
    # From the merge base it answers the only question that matters: what did
    # UPSTREAM change in the range we are about to merge?
    mb=$(git merge-base HEAD upstream/main)
    if ! git diff --quiet "$mb..upstream/main" -- Cargo.toml Cargo.lock crates/; then
      echo "- **[!] crate/workspace or dependency change** — a new or moved crate re-homes our command variants; a version bump in \`Cargo.toml\` must agree with \`Cargo.lock\` after the merge:"
      echo '```'
      git diff --stat "$mb..upstream/main" -- Cargo.toml Cargo.lock crates/
      echo '```'
      hit=1
    fi
    # Field added to a struct our fork extends (EventResponse.activate,
    # Column.tabbed/.active) — every such field forces a sweep of our literals.
    fields=$(git log -p HEAD..upstream/main -- src/layout_engine/engine.rs \
      src/actor/reactor/events/outcome.rs src/layout_engine/systems/scrolling.rs 2>/dev/null \
      | grep -cE '^\+ +pub (changed|activate|tabbed|active|width_overridden|boundary_hit|raise_windows|focus_window)\b')
    if [ "${fields:-0}" -gt 0 ]; then
      echo "- **[!] $fields added field(s)** on \`EventResponse\`/\`EventOutcome\`/\`Column\` — expect a literal sweep (FORK.md §4 items 2 and 3)."
      hit=1
    fi
    # `grep -c`, never `grep -q`: with `pipefail`, a short-circuiting `grep -q`
    # SIGPIPEs the upstream `git log -p` and the pipeline returns 141, so a real
    # match reported "no hit". This check silently never fired until 2026-10-16.
    sigs=$(git log -p HEAD..upstream/main -- src/layout_engine/ 2>/dev/null \
      | grep -cE '^[-+].*fn (calculate_layout|update_layout|create_layout)\(')
    if [ "${sigs:-0}" -gt 0 ]; then
      echo "- **[!] \`calculate_layout\`/\`update_layout\`/\`create_layout\` signature changed** ($sigs hunk line(s)) — our hint-bar reserve recursion threads these (FORK.md §2.E)."
      hit=1
    fi
    # UPSTREAM-SYNC.md §12.10 item 2: never skip a second upstream minor. Tags are
    # upstream's own statement that something landed; two minors behind means a
    # rewrite has probably happened. Objective, and needs no diff reading.
    ours=$(git describe --tags --abbrev=0 HEAD 2>/dev/null | sed 's/^v//')
    theirs=$(git describe --tags --abbrev=0 upstream/main 2>/dev/null | sed 's/^v//')
    if [ -n "$ours" ] && [ -n "$theirs" ]; then
      om=$(echo "$ours" | cut -d. -f2); tm=$(echo "$theirs" | cut -d. -f2)
      oM=$(echo "$ours" | cut -d. -f1); tM=$(echo "$theirs" | cut -d. -f1)
      if [ "$tM" -gt "$oM" ] || { [ "$tM" -eq "$oM" ] && [ $((tm - om)) -ge 2 ]; }; then
        echo "- **[!] release ceiling breached** — we are on \`$ours\`, upstream is on \`$theirs\` (two or more minors)."
        hit=1
      fi
    fi
    for f in src/layout_engine/systems/scrolling.rs src/actor/reactor.rs src/actor/app.rs; do
      n=$(git log --oneline HEAD..upstream/main -- "$f" | wc -l | tr -d ' ')
      [ "$n" -gt 0 ] && echo "- [.] $n commit(s) touch \`$f\` (our churniest shared files)"
    done
    if [ "$hit" -eq 0 ]; then
      echo "- no structural tripwire — a batched merge is fine (still checkpoint it)."
      exit 0
    fi
    exit 1

# Deliberately a merge, not a rebase: §3 rejected rebasing because it replays our
# commits against every upstream step and re-resolves the same conflicts; merge
# resolves each once and `rerere` records it. Rebase is for upstreaming a PR only.
#
# Merges the WHOLE RANGE in one go rather than commit-by-commit. That is not
# laziness, it is the cheaper conflict set: git's 3-way merge over a range absorbs
# upstream's internal churn, including commits it later reverts. Measured on
# b31dddf..7829780 — range merge: 2 additive hunks. Per-commit: `1b69d8e` alone
# conflicts across 7 files and rewrites `Request::Raise` to carry
# `FocusConfirmation` where our fork carries `activate: bool`, and then `7829780`
# reverts the whole thing. You would resolve the fork's most delicate feature across
# 7 files to reach exactly where you started.
#
# Too big to resolve in one bite? Pick a checkpoint SHA and use `just sync-to <sha>`
# (UPSTREAM-SYNC.md §6 did the 38-commit backlog as 5 hand-picked checkpoints).
# merge upstream in one range merge, gated on build + tests
sync: (sync-to "upstream/main")

# merge upstream up to a specific checkpoint sha (for a backlog worth splitting)
sync-to target:
    #!/usr/bin/env bash
    set -uo pipefail
    git fetch upstream --quiet
    if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
      echo "working tree dirty — commit or stash first"; exit 1
    fi
    behind=$(git rev-list --count HEAD..{{target}} 2>/dev/null) || {
      echo "unknown target: {{target}}"; exit 1; }
    if [ "$behind" -eq 0 ]; then echo "already up to date with {{target}}."; exit 0; fi
    echo "=== merging $behind commit(s) up to {{target}} ==="
    git log --oneline --no-merges HEAD..{{target}} | sed 's/^/  /'
    git merge --no-edit {{target}}
    # NOTE: exit code is unreliable here — `git merge` still reports failure when
    # rerere replays a resolution, even though it left the file clean. Ask git what
    # is actually unmerged instead of trusting $?, then split that list: a path can
    # be unmerged in the INDEX while already conflict-free in the WORKING TREE,
    # which is exactly what a rerere replay looks like. Reporting those as "resolve
    # this" sends you to inspect a file that has nothing left to fix.
    unmerged=$(git diff --name-only --diff-filter=U)
    if [ -n "$unmerged" ]; then
      auto=""; manual=""
      while IFS= read -r f; do
        if grep -q '^<<<<<<<' "$f" 2>/dev/null; then manual="${manual}${f}"$'\n'
        else auto="${auto}${f}"$'\n'; fi
      done <<< "$unmerged"
      echo
      if [ -n "$auto" ]; then
        echo "rerere replayed a known resolution for $(printf '%s' "$auto" | grep -c .) file(s) —"
        echo "already conflict-free, but REVIEW before staging (upstream may have moved"
        echo "the surrounding code since the resolution was recorded):"
        printf '%s' "$auto" | sed 's/^/  /'
      fi
      if [ -n "$manual" ]; then
        echo "needs manual resolution ($(printf '%s' "$manual" | grep -c .) file(s)):"
        printf '%s' "$manual" | sed 's/^/  /'
      fi
      echo
      echo "This is the MINIMAL conflict set for this range — do not try to shrink it"
      echo "by merging commit-by-commit, that is usually strictly worse (see comment)."
      echo "Resolve (playbook: FORK.md §4), then:"
      echo "  git add -A && git commit && just check && just test"
      exit 1
    fi
    if ! git diff --quiet --cached 2>/dev/null || ! git diff --quiet 2>/dev/null; then
      git commit --no-edit >/dev/null 2>&1 || true
    fi
    echo "=== merged cleanly; gating on build ==="
    # `nix develop -c`, not bare `cargo`: --all-targets builds the test targets, which
    # LINK against the SDK, and cargo itself is only on PATH inside the dev shell (or
    # via direnv in the repo dir). A bare `cargo` here works in the blessed checkout
    # and fails in every git worktree — the same trap FORK.md §6 records for
    # `just fmt`. Run once and reuse the output; a second check just pays twice.
    out=$(nix develop {{justfile_directory()}} -c \
          cargo check --workspace --all-targets --message-format=short 2>&1)
    errs=$(printf '%s\n' "$out" | grep -c 'error\[\|^error:')
    if [ "${errs:-0}" -gt 0 ]; then
      printf '%s\n' "$out" | grep -E 'error' | head -20
      echo "BUILD BROKE ($errs error(s)) — fix on top of the merge, then re-run gates"
      exit 1
    fi
    echo "=== build clean; running tests ==="
    just test

# Answers "our regression or theirs" without guessing — the question every red test
# during a sync raises. Builds a detached worktree at upstream/main, runs TEST there.
# e.g. `just upstream-test ax_invalidation_after_quarantine_release_preserves_live_layout_state`
# does TEST also fail on pristine upstream? (our regression, or upstream's)
upstream-test TEST:
    #!/usr/bin/env bash
    set -uo pipefail
    git fetch upstream --quiet
    wt=$(mktemp -d -t rift-pristine)
    trap 'git worktree remove --force "$wt" >/dev/null 2>&1 || true' EXIT
    git worktree add -f --detach "$wt" upstream/main >/dev/null 2>&1
    echo "=== {{TEST}} on pristine upstream/main ($(git rev-parse --short upstream/main)) ==="
    # --nocapture matters: panics abort instead of unwinding in this test binary, and
    # without it the assertion message is swallowed and you only learn WHICH test died.
    (cd "$wt" && nix develop {{justfile_directory()}} -c \
      cargo test --lib -- {{TEST}} --test-threads=1 --nocapture 2>&1 | tail -20)

# ---------------------------------------------------------------------------
# local config — test config changes without a darwin-rebuild
# ---------------------------------------------------------------------------
#
# The launchd agent is a PLAIN FILE (home-manager writes it, but nothing keeps
# it a store symlink), so its `--config` argument can be repointed at a file in
# this checkout. That is the whole trick: no rebuild, no system generation.
#
# There is deliberately NO automatic seeding. A silent copy of the live config
# rots: you fix something here, nix never learns about it, and six weeks later
# the two disagree and nobody knows which is right. `seed-local-config` is an
# explicit one-off, and `diff-config` exists so the drift stays visible.
#
# `darwin-rebuild switch` rewrites the plist and silently puts you back on the
# system config. That is the escape hatch, not a bug — but it is also why
# `status` prints which config is actually in force.

# point the running agent at ./local-config.toml
use-local-config:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -f "{{local_config}}" ]; then
      echo "no {{local_config}}"
      echo "create one, or copy the live config once with: just seed-local-config"
      exit 1
    fi
    current=$(/usr/libexec/PlistBuddy -c "Print :ProgramArguments:2" "{{plist}}")
    if [ "$current" != "{{local_config}}" ]; then
      echo "==> repointing the launchd agent"
      echo "    --config $current"
      echo "    --config {{local_config}}"
      # Remember where to go back to; a rebuild changes the store path, and a
      # rebuild also rewrites the plist, so this only has to survive until then.
      echo "$current" > "{{justfile_directory()}}/.local-config-previous"
      /usr/libexec/PlistBuddy -c "Set :ProgramArguments:2 {{local_config}}" "{{plist}}"
    else
      echo "==> already on the local config"
    fi
    # NOT `just restart`: `launchctl kickstart -k` restarts from launchd's
    # CACHED job definition and keeps the old --config, so the switch looks
    # like it did nothing. The plist has to be unloaded and re-bootstrapped.
    just stop
    just start
    echo "✓ rift now reads ./local-config.toml — edit it and \`just restart\`"
    echo "  NOTE: \`darwin-rebuild switch\` rewrites this plist and reverts you."

# put the agent back on the home-manager config
use-system-config:
    #!/usr/bin/env bash
    set -euo pipefail
    prev_file="{{justfile_directory()}}/.local-config-previous"
    if [ ! -f "$prev_file" ]; then
      echo "no record of the previous config path (.local-config-previous missing)."
      echo "run \`darwin-rebuild switch\` — it rewrites the plist to the system config."
      exit 1
    fi
    prev=$(cat "$prev_file")
    echo "==> restoring the agent to the home-manager config"
    echo "    --config $(/usr/libexec/PlistBuddy -c "Print :ProgramArguments:2" "{{plist}}")"
    echo "    --config $prev"
    /usr/libexec/PlistBuddy -c "Set :ProgramArguments:2 $prev" "{{plist}}"
    # NOT `just restart`: `launchctl kickstart -k` restarts from launchd's
    # CACHED job definition and keeps the old --config, so the switch looks
    # like it did nothing. The plist has to be unloaded and re-bootstrapped.
    just stop
    just start
    echo "✓ back on the system config; ./local-config.toml left on disk"

# one-off: copy whatever config the agent currently uses into the checkout
seed-local-config:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -f "{{local_config}}" ]; then
      echo "{{local_config}} already exists — refusing to overwrite."
      echo "see what differs from the system config with: just diff-config"
      exit 1
    fi
    src=$(/usr/libexec/PlistBuddy -c "Print :ProgramArguments:2" "{{plist}}")
    echo "==> seeding from $src"
    install -m 644 "$src" "{{local_config}}"
    echo "✓ wrote {{local_config}} ($(wc -l < "{{local_config}}" | tr -d ' ') lines)"
    echo
    echo "  This is now a COPY. Anything you fix here is invisible to nix until"
    echo "  you port it back — \`just diff-config\` is how you see what is owed."

# what has ./local-config.toml drifted from the system config?
diff-config:
    #!/usr/bin/env bash
    set -uo pipefail
    if [ ! -f "{{local_config}}" ]; then echo "no {{local_config}}"; exit 1; fi
    prev_file="{{justfile_directory()}}/.local-config-previous"
    if [ -f "$prev_file" ]; then
      sys=$(cat "$prev_file")
    else
      sys=$(/usr/libexec/PlistBuddy -c "Print :ProgramArguments:2" "{{plist}}")
    fi
    echo "system: $sys"
    echo "local : {{local_config}}"
    echo
    if diff -u "$sys" "{{local_config}}" --label system --label local; then
      echo "(identical — nothing owed back to nix)"
    else
      echo
      echo "^ every line above is a local-only change nix does not have yet."
    fi
