# Fork maintenance guide

This is a **fork** of [`acsandmann/rift`](https://github.com/acsandmann/rift) that
carries a small set of local changes on top of upstream. This document explains
what we add, why, and — most importantly — how to keep pulling upstream updates
without pain.

Read this before syncing with upstream or adding new local features.

---

## 1. Repository topology

| Remote | URL | Role |
|---|---|---|
| `origin` | `git@github.com:xieyt/rift.git` | our fork (push here) |
| `upstream` | `git@github.com:acsandmann/rift.git` | source of truth (never push here) |

Branches:

- **`udpate/to-latest`** — the working branch. Upstream's `main` + our local
  changes, rebased on top. This is what we build and ship.
  *(The name is a typo for `update/to-latest`; kept for continuity. Rename with
  `git branch -m` + re-push if it ever bothers you.)*
- **`main`** — kept in sync with `upstream/main` as a clean mirror. No local work
  lands here.

> **Rule:** local work never touches upstream. We only ever push to `origin`.
> The original developer only sees what we deliberately open as a PR from a
> dedicated feature branch — never this branch.

---

## 2. What this fork adds

Everything we carry lives in **three single-purpose commits** on top of
`upstream/main`. Keeping it to three tight commits is deliberate (see §5).

### A. `feat(nix)` — packaging & distribution
**Files:** `flake.nix`, `flake.lock`, `nix/{module,overlay,package}.nix`,
`.github/workflows/release.yml`, `.gitignore`, `Cargo.toml` (adds an explicit
`[[bin]] rift-cli`).

- Builds rift from source and wraps the binaries in a macOS `.app` bundle so
  TCC / accessibility permissions and the codesign identity survive
  `darwin-rebuild`.
- NixOS / nix-darwin module exposing `config`, `logDir`, `logLevel`, and an
  hourly log-rotation agent.
- Slim shared toolchain: the crane build **and** the flake devShell use one
  `fenix.combine` of cargo/rustc/rust-std/clippy/rustfmt — dropping rust-docs
  (~705 MB), rust-analyzer, and rust-src. ~864 MB closure vs ~1.6 GB for the
  full `stable.toolchain`. The devShell also brings a matched macOS SDK +
  libiconv, so ambient `cargo`/`just` link inside it (see §6).

**Why:** upstream ships no Nix support at all.
**Conflict risk:** ~zero. These are almost entirely *new files* upstream doesn't
have; only `Cargo.toml` can ever collide.

### B. `feat: app activation on focus`
**Files:** `src/actor/app.rs`, `src/actor/raise_manager.rs`,
`src/actor/reactor.rs`, `src/layout_engine/engine.rs`, `src/bin/rift-cli.rs`,
`src/sys/app.rs`, `src/actor/reactor/events/{command,window}.rs`,
`src/common/config.rs`.

- Threads an `activate: bool` flag through the entire raise pipeline:
  `Request::Raise` / `RaiseRequest` (app.rs) → `raise_manager` → `reactor`
  → `LayoutCommand::MoveFocus(MoveFocusArgs { direction, activate })` → `EventResponse.activate`.
- Adds `NSRunningApplication::activate()` (`sys/app.rs`) and the
  `rift-cli execute window focus <dir> --activate` CLI flag.
- **Single policy:** `[settings.layout] activate_on_focus` (default `true`) gates
  *both* keyboard focus moves and workspace switches; the per-command `activate`
  flag overrides it upward. Set it `false` to require the explicit flag.
- **Config back-compat (CRITICAL):** `MoveFocus` carries `activate`, but it
  deserializes through a custom `MoveFocusArgs` impl (`engine.rs`) that accepts
  *both* the bare `move_focus = "left"` (upstream's default-config form;
  `activate` defaults `false`) *and* the table
  `move_focus = { direction = "left", activate = true }`. Without the dual-form
  parse, `Config::default()` — which parses the embedded `rift.default.toml` —
  panics, so app startup and every test that uses defaults abort. `cargo check`
  does **not** catch this (it never runs `Config::default()`); `just test` does.
  If you touch `MoveFocus`, keep both forms parseable.

**Effect:** a focus move / workspace switch brings the target *app* to the
foreground, not just shifting keyboard focus.
**Why:** upstream only moves keyboard focus; it never foregrounds the app.
**Note:** `sys/app.rs` intentionally uses the deprecated
`activateWithOptions(ActivateIgnoringOtherApps)` under `#[allow(deprecated)]` — it
gives the forceful foregrounding a WM wants, and we target current macOS, so the
deprecation is irrelevant here.
**Conflict risk:** HIGH — touches ~7 core files upstream changes constantly. Kept
as a *single* commit so you resolve it once per upstream bump, not once per
sub-change.

### Dropped / superseded (do NOT re-add)
- **scrolling off-left snap** — a per-column `x` clamp in `scrolling.rs`. Reverted:
  upstream already clamps the scroll offset, so it was a redundant heuristic in
  upstream's most-churned file. Reverted to match upstream exactly.
- **fallback-focus rename**, **layout-focus-on-activation**, and a dead
  `raise_window` helper — all superseded by upstream's declarative `EventOutcome`
  model (`outcome.focused_window`). See §5.

---

## 3. Syncing with upstream (the routine)

```bash
git fetch upstream

# keep the clean mirror in sync (fast-forward, never conflicts)
git push origin upstream/main:main

# rebase our work onto the new upstream tip
git checkout udpate/to-latest
git rebase upstream/main
# ...resolve conflicts if any (see §4), then:
cargo check                       # must pass
git push --force-with-lease origin udpate/to-latest
```

`--force-with-lease` is required because rebasing rewrites our commits. It is
safe here: this is our branch on our fork, and it refuses to clobber unexpected
remote changes.

---

## 4. Conflict-resolution playbook

Conflicts land almost exclusively in **feature B** (§2). The pattern is always
the same: upstream refactors a core file, and our `activate` threading collides.

Resolution principles that worked last time:

1. **Take upstream's structure, re-apply our `activate` thread.** When upstream
   renames variables, changes a function signature, or moves code, adopt *their*
   version and just re-add the `activate` field / argument on top.
2. **`EventResponse` literals need `activate: false`.** Upstream adds
   `EventResponse { ... }` sites frequently; every one needs our `activate`
   field. Missing field → `E0063`.
3. **`RaiseRequest` literals need `activate: false`** too (in
   `reactor/events/command.rs`, `reactor/events/window.rs`, and anywhere upstream
   constructs a raise).
4. **`move_focus_internal` takes `window_store` as its first arg** upstream —
   keep that; our change is only the `activate` handling around the call.
5. **`make_key_result` is `Option<Result<..>>`** — use
   `.as_ref().is_some_and(Result::is_ok)`, never `.is_ok()` directly.
6. **When our old code duplicates something upstream now provides, drop ours.**
   Upstream moved to a declarative "return `EventOutcome`, let the reactor apply
   it" pattern. Old imperative helpers of ours are dead weight — delete them and
   rely on upstream's path (e.g. layout focus sync now flows through
   `outcome.focused_window`).

After resolving: `git add -A && git rebase --continue`, then **always**
`cargo check`.

---

## 5. Maintenance philosophy (why the history looks like this)

Rebase pain scales with **(core-file lines we touch) × (commits spreading those
touches)**. So:

- **One commit per feature, never many.** A feature split across N commits editing
  the same lines makes you resolve the same conflict N times. Squash before you
  push a new feature.
- **Quarantine packaging from behavior.** The Nix commit touches only new files,
  so it never conflicts — keep it separate and mentally skip it during merges.
- **Keep micro-fixes as their own tiny commits** so they're trivial to drop the
  moment upstream supersedes them.
- **Drop anything upstream has absorbed.** Before defending a local patch, check
  whether upstream already does it. Two fixes were removed this way:
  a superseded `response → state_response` rename, and a layout-focus-on-activation
  tweak upstream now handles via `outcome.focused_window`. Dead code
  (an unused `raise_window` helper) was also removed.

If you add a new local feature: build it, squash it into one clean commit with a
descriptive message, keep it above the Nix commit, and update §2 here.

---

## 6. Building & verifying

### Dev environment (do this first)

The Rust toolchain, macOS SDK, and libiconv all come from Nix. Load them so
ambient `cargo`/`just` link:

```bash
direnv allow          # once; `.envrc` runs `use flake` (needs nix-direnv)
# or, without direnv:
nix develop           # same environment, entered manually
```

In a *plain* shell (no direnv, not in `nix develop`) only the link-free checks
work; a full `cargo build`/`cargo test` fails with `ld: library not found for
-liconv` because the wrong SDK is active. That's environment, not code.

```bash
cargo check          # fast type/borrow check — use during conflict resolution
cargo build --lib    # compiles the library
nix build            # real end-to-end .app build (first run pulls the toolchain)
```

### Tests

```bash
just test            # fast deterministic logic tests (our coverage); no GUI
just test-all        # whole lib suite — only in a real GUI session
```

`just test` runs through `nix develop` so it links even without direnv. Three
things to know:

- **One known upstream failure:** `topology_change_clears_stale_pending_hide_target_before_next_workspace_layout`
  panics on clean `upstream/main` too (verified). `just test` skips it — it is
  not ours, don't chase it.
- **Window-server tests need a real GUI session.** Tests touching SkyLight
  (`SLS…`) abort with an objc weak-reference error in a headless/agent shell,
  so `just test` excludes them; use `just test-all` from your logged-in desktop.
- Panics in the test binary currently *abort* instead of unwinding
  (`failed to initiate panic`), so one failing test kills the whole run — which
  is why `just test` uses a curated allowlist rather than the full suite.

**Install / test on this Mac.** Real builds and the running WM are driven via
`just` (see the `justfile`):

```bash
just setup-signing-cert   # one-time (admin): stable self-signed cert so the
                          #   Accessibility grant survives rebuilds
just install              # build .#rift, swap /Applications/Rift.app, restart
just smoke                # build raw binaries and prove they link + run
just logs                 # tail rift logs
```

Without `setup-signing-cert`, installs are ad-hoc signed and macOS drops the
Accessibility grant on every rebuild (you'd re-grant + `just restart` each time).
With the cert, TCC keys on the stable signing identity and the grant persists.

**Installing via the flake** (`darwin-rebuild switch` with `services.rift.enable`)
needs *no* manual cert setup: the module auto-creates the `rift-codesign` identity
on first activation (`services.rift.manageSigningIdentity`, default `true`) and
signs with it, so Accessibility persists across rebuilds out of the box. The
`just setup-signing-cert` step above is only for the local `just install` dev loop.
Note: rift only manages *activated* spaces — if focus/layout commands are ignored,
run `rift-cli execute toggle-space-activated` (or your `hyper+Z` bind).

---

## 7. Quick reference

```bash
# how far are we from upstream?
git fetch upstream
git rev-list --left-right --count upstream/main...udpate/to-latest
#  -> "<N behind>  <M ahead>"   ; M = number of commits we carry on top

# what exactly do we carry on top of upstream?
git log --oneline upstream/main..udpate/to-latest
git diff --stat upstream/main..udpate/to-latest
```
