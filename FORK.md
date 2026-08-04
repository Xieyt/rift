# Fork maintenance guide

This is a **fork** of [`acsandmann/rift`](https://github.com/acsandmann/rift) that
carries a small set of local changes on top of upstream. This document explains
what we add, why, and — most importantly — how to keep pulling upstream updates
without pain.

Read this before syncing with upstream or adding new local features.

> **Currently behind upstream?** See `UPSTREAM-SYNC.md` for the per-commit
> verdicts and the staged merge plan for the 2026-08 backlog (38 commits).

---

## 1. Repository topology

| Remote | URL | Role |
|---|---|---|
| `origin` | `git@github.com:xieyt/rift.git` | our fork (push here) |
| `upstream` | `git@github.com:acsandmann/rift.git` | source of truth (never push here) |

Branches:

- **`xieyt/4`** — the working branch: `upstream/main` **merged** with our local
  changes on top. This is what we build and ship. (Older notes here called it
  `udpate/to-latest`; the live branch is `xieyt/4`.)
- **`main`** — kept in sync with `upstream/main` as a clean mirror. No local work
  lands here.

> **Rule:** local work never touches upstream. We only ever push to `origin`.
> The original developer only sees what we deliberately open as a PR from a
> dedicated feature branch — never this branch.

---

## 2. What this fork adds

Everything we carry lives in a **handful of single-purpose commits** on top of
`upstream/main` — one per feature, kept tight on purpose (see §5).

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
  - **rustfmt is pinned to nightly** (`latest.rustfmt`), not stable: `rustfmt.toml`
    enables unstable options, so stable `cargo fmt` would silently reflow the
    whole tree. See §6 "Formatting".

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

### C. `feat: tabbed column display`
**Files:** `src/common/config.rs`, `src/actor/stack_line.rs`,
`src/actor/reactor/managers.rs`, `src/ui/stack_line.rs`, `rift.default.toml`.

- Renders window **titles as tab labels** on the existing stack-line indicator
  (niri-style tabbed columns). Rift's stack layout already collapses a group to
  one frame and draws a clickable segmented bar; this threads per-window titles
  (`GroupInfo` → `GroupDisplayData`) from the window registry through the
  `stack_line` actor and draws a centered `CATextLayer` per segment, reusing
  `calculate_segment_frame` so labels line up with the selected-segment
  highlight and the click hit-testing.
- Opt-in: `[settings.ui.stack_line] show_titles` (default `false`); wants a
  larger `thickness` (20+) to fit text.
- **Render fix + theming:** the indicator CGS window is `setGeometryFlipped(true)`
  (like `ui/mission_control.rs`) so titles draw upright, not mirrored. Colors and
  size are configurable under `[settings.ui.stack_line]`: `active_color`,
  `inactive_color`, `active_title_color`, `inactive_title_color` (hex
  `#RRGGBB[AA]`) and `font_size`; unset → dark bar + accent-blue active tab.
- **Debug logging** for the indicator pipeline (`managers` group feed, actor
  event/enable state, positioned frames) is kept at `debug!` and enabled by
  default for the fork via `nix` `logLevel` (env RUST_LOG), not a config flag.

**Why:** turns the minimalist stack indicator into readable tabs — niri's
space-saver — with no new layout plumbing (pure display variant).
**Conflict risk:** LOW–MEDIUM — `ui/stack_line.rs` + the `stack_line` actor + one
`managers.rs` map; not upstream's most-churned area.
**Verify:** `cargo check` + `just test` (geometry unit test
`tab_segments_tile_the_bar_without_gaps_or_overlap`). Visuals need `just install`
on a real Mac — SkyLight indicator windows can't render in a headless session.

### D. `feat: niri tabbed columns (scrolling)`
**Files:** `src/layout_engine/systems/scrolling.rs`,
`src/layout_engine/systems.rs`, `src/layout_engine/engine.rs`, `src/bin/rift-cli.rs`.

- Adds a per-column **tabbed display** to the scrolling (niri) layout: a
  `Column.tabbed` flag makes all of a column's windows share one full-height
  frame (overlap, focused on top) instead of tiling vertically. Focus is
  unaffected — up/down still cycles the column's windows (switching tabs),
  left/right moves between columns.
- Feeds tabbed columns into the **same** stack-line indicator pipeline as §C: a
  new `LayoutSystem::toggle_selection_tabbed` (default no-op) plus
  `ScrollingLayoutSystem::collect_group_containers_scrolling`, which caches the
  authoritative column frame during `calculate_layout` (`tab_groups`) and
  synthesizes a stable `NodeId` per column (`KeyData::from_ffi`). So with §C's
  `show_titles` on, scrolling tabs get titles too.
- Toggle: `LayoutCommand::ToggleColumnTabbed` → `rift-cli execute layout toggle-tabbed`
  (or bind `toggle_column_tabbed`). Build a multi-window column first
  (`consume-or-expel-window`), then toggle tabbed.
- **Bar not occluded:** tabbed windows are inset below a reserved
  `stack_line.thickness` strip, so the `order_below` indicator isn't covered by
  the focused window (this was the "bar doesn't show" bug).
- **Tab memory:** each `Column` tracks its last-focused window (`active`), so
  `move_focus_horizontal` restores a column's active tab on return instead of
  snapping to the source column's row. Test:
  `horizontal_focus_restores_active_tab_in_column`.

**Why:** the *real* niri tabbed-columns feature — §C only decorated the tree/stack
indicator, which the scrolling layout never emits.
**Conflict risk:** MEDIUM–HIGH — touches `scrolling.rs`, upstream's most-churned
file. Kept minimal: one `Column` field, one `calculate_layout` branch, one
indicator method, one trait default, one engine arm + command.
**Verify:** `just test` (`tabbed_column_overlaps_windows_and_emits_one_group`,
`toggle_selection_tabbed_flips_the_column_flag`). Visuals need `just install`.

### E. `feat: scrolling-strip hint bar`
**Files:** `src/ui/hints_bar.rs` (new), `src/actor/hints_bar.rs` (new),
`src/ui.rs`, `src/actor.rs`, `src/common/config.rs`, `rift.default.toml`,
`src/actor/reactor.rs`, `src/actor/reactor/managers.rs`,
`src/actor/wm_controller.rs`, `src/actor/event_tap.rs`, `src/bin/rift.rs`,
`src/model/reactor.rs`.

- A bottom (or top) **hint bar** for the scrolling (niri) layout: a centered
  capsule of per-column chips (`keycap · app-icon · app-name`), focused column
  filled with the accent, on-screen columns grouped under a brighter viewport
  tint, off-screen columns dimmed. Multi-window columns get a count badge; the
  focused multi-window column gets a **detail overlay** listing each
  window's icon + name (active highlighted). Click a chip to focus that column
  (routes through the existing `ReactorCommand::FocusWindow`).
- Own subsystem mirroring `stack_line`: a `hints_bar` actor owns one CGS window
  (+ a second window for the detail overlay); the reactor feeds a snapshot per
  layout apply from `managers.rs` (`build_hints_bar_cells`), grouping columns
  from window frames by x and pulling tab/stack detail from the engine's
  `tab_groups` (`collect_group_containers`). Independent of `stack_line` —
  `[settings.ui.hints_bar] enabled` and `[settings.ui.stack_line] enabled` are
  separate flags; run either, both, or neither.
- Config `[settings.ui.hints_bar]`: `enabled`, `position` (`bottom`/`top`
  horizontal, `left`/`right` vertical — vertical stacks chips down an edge and
  is overlay-only), `placement` (`overlay` floats over windows / `reserve`
  shrinks the scrolling tiling area, horizontal only), `visibility` (`always` /
  `on_demand` / `auto` flash), `density` (compact/full/dots), `height` (chip
  thickness / row height), `auto_hide_ms`, `keys`, hex colors, `pad` (per-side
  `{ top, bottom, left, right }` px insetting the whole bar — shared `HintsBarPad`),
  `align` (`start`/`center`/`end` along the edge), and `notch` (`flow` = the
  crossing chip stretches across the notch with its label continuing past it /
  `stop` = end before it / `ignore`; only acts when a top
  bar sits on the notch row). The focused-column
  detail is its own sub-table `[settings.ui.hints_bar.detail]`: `style` =
  `popover` (card next to the focused chip) or `bar` (a second hint-bar strip of
  the column's windows). For `bar`: `position` (`bottom`/`top` horizontal,
  `left`/`right` vertical chip stack), `align` (`start`/`center`/`end` along that
  edge — `end` on a shared edge pushes the column bar to the opposite side), and
  `pad` (per-side `{ top, bottom, left, right }` px from the display edges).
  Honors `show_titles`.
  Hot-reloadable. Toggle command `toggle_hints_bar` (`WmCmd` →
  `ReactorCommand::ToggleHintsBar`).
- **Workspace overview rail** (`show_workspace = true`): leading pills for each
  *occupied* virtual workspace — number + an icon per distinct app open in it,
  active one filled. Click a pill to switch (`SwitchToWorkspace`). Data:
  `LayoutEngine::workspace_app_summaries` → `WorkspaceStore::workspace_app_pids`.
- **Keyboard column jump:** `LayoutCommand::FocusColumn(n)` focuses the Nth
  column left→right (matching the bar's hint letters), revealing it like
  `move_focus`. Bind per key (`{ focus_column = 0 }`) or `rift-cli execute
  layout focus-column N`; `keys` supplies the on-bar labels.
- **Overflow** (`max_width_ratio` + `overflow`): cap the chip row at a fraction
  of display width; the excess wraps per `overflow` — `scale` (compress, the
  default = old behavior), `row` (second row on a taller top bar), or `bar` (a
  second CGS strip on the opposite edge). The rail + fitting columns stay on top;
  a click on either strip maps to the right window (the overflow strip renders
  the tail *slice*, so no index math). Split point is `HintsBar::overflow_split`
  (pure). Under `placement = reserve` the tiling area shrinks for the extra
  strip **dynamically** (only while overflowing) via a one-pass reserve-feedback
  loop in `managers.rs` (`hints_bar_reserve` + a guarded convergence re-layout in
  `update_layout`).
- **Crisp rendering (CRITICAL):** the CGS window + root layer must set
  `contentsScale` **and** `set_resolution` to the display `backingScaleFactor`
  (like `ui/mission_control.rs`), else the layer tree rasterizes at 1x and
  upscales on Retina (grainy). The `auto`-flash timer uses `sys::timer::Timer`
  (CFRunLoop), **not** `tokio::time` — actors run on a custom CFRunLoop executor
  with no tokio runtime, so `tokio::time::sleep` panics at runtime.
- **Performance (unconditional):** the bar only re-renders when its content
  actually changes — reactor-side snapshot **dedup** (skip identical), a 12 ms
  **debounce** (coalesce bursts, e.g. a scroll gesture), **render-gating** (skip
  unchanged so idle/redundant applies do nothing), and a per-pid **icon cache**
  (icons resolved once, not re-rasterized each render). No config knob — these
  are always on. `trace!(target: "hbbench", …)` counters (`hb_sent`/`hb_render`)
  are left in for benchmarking; see §6 "Benchmarking a perf change". A naive
  build re-rendered on every layout apply (~1,649 renders for 100 focus moves);
  this is ~15× fewer.
- **External-bar reservation:** `[settings.layout] external_bar_top` /
  `external_bar_bottom` (px) inset the tiling frame at the top/bottom so tiled
  windows never sit under an external bar like **sketchybar** — every layout,
  independent of the hint bar (`managers.rs` shrinks `tiling_frame`). Pair with a
  `hints_bar` `placement = "overlay"` + `align = "start"` to float rift's bar on
  the left of that strip while sketchybar owns the right.

**Why:** a niri-style "where am I in the strip + what's in each column" HUD; it
also supersedes the stack-line tab bar for scrolling (turn `stack_line` off).
**Conflict risk:** LOW — almost all new files; touched core files get small,
localized additions (one feed block, one command arm, one actor spawn, one
event-tap click route).
**Verify:** `just test` (`ui::hints_bar` geometry/click tests). Visuals need
`just install` on a real Mac (SkyLight windows can't render headless).

### Dropped / superseded (do NOT re-add)
- **scrolling off-left snap** — a per-column `x` clamp in `scrolling.rs`. Reverted:
  upstream already clamps the scroll offset, so it was a redundant heuristic in
  upstream's most-churned file. Reverted to match upstream exactly.
- **fallback-focus rename**, **layout-focus-on-activation**, and a dead
  `raise_window` helper — all superseded by upstream's declarative `EventOutcome`
  model (`outcome.focused_window`). See §5.

---

## 3. Syncing with upstream (rebase vs merge)

**Use `merge`, not `rebase`, for routine syncs.** We carry a large feature set
(~26 commits, see §2) and sync periodically, so upstream deltas are non-trivial.
A merge resolves the overlap **once** per sync, keeps commit hashes stable (no
force-push, safe for a shared branch), and — with `git rerere` — replays a
resolution you've already done. Rebase only wins if you sync *very* frequently
(tiny deltas) or want a linear history to open an upstream PR; then force-pushing
the fork branch is expected. That is **not** our normal flow — rebasing ~26
commits over a real upstream delta is a per-commit conflict marathon; merging is
one pass.

`git rerere` is enabled (`git config rerere.enabled true`) so each conflict you
resolve is recorded and auto-applied next time. Do **not** count on it across a
large delta: verified 2026-08-05, none of the 3 resolutions in `.git/rr-cache`
matched any of the 16 hunks in the 38-commit backlog — upstream had moved the
surrounding code. rerere pays off for repeated *small* syncs, which is the
argument for syncing often (`UPSTREAM-SYNC.md` §7).

**Routine (merge):**

```bash
git fetch upstream

# keep the clean mirror in sync (fast-forward, never conflicts)
git push origin upstream/main:main

git checkout xieyt/4                 # our working branch
git merge upstream/main
# ...resolve conflicts if any (see §4), then:
git add -A
cargo build --lib                    # must compile (catches merged-symbol clashes)
just test                            # our curated suite must pass
git commit                           # finalize the merge commit
git push origin xieyt/4              # no --force needed
```

**Rebase alternative** (only for upstreaming / a linear history):

```bash
git checkout xieyt/4
git rebase upstream/main             # resolve per-commit; rerere still helps
git push --force-with-lease origin xieyt/4
```

---

## 4. Conflict-resolution playbook

Conflicts land in the files our churny features touch. Historically that's
**feature B**'s `activate` threading; the last sync instead hit the **layout
engine** (upstream refactored persistence into `engine/persistence/` and added
restore/query APIs).

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

After resolving: `git add -A`, then `cargo build --lib` — **merge:** `git commit`;
**rebase:** `git rebase --continue`. Always build before committing.

### Recorded resolutions (rerere has these)

The last sync conflicted **only in the layout engine**. If they recur, `rerere`
replays them; the intended resolutions are:

- **`src/layout_engine.rs`** re-export → the **union**: keep our `MoveFocusArgs`
  *and* upstream's `RestoreReport`/`RestoreRequest`/`RestoreScope`/`RestoreSource`/
  `RestoreWarning`.
- **`src/layout_engine/engine.rs`** → **drop** our `serialize_to_string`; upstream
  moved it to `engine/persistence/storage.rs`. Keeping ours is a duplicate
  definition *and* won't compile (`LayoutEngine` no longer derives `Serialize`).
- **`src/layout_engine/systems/scrolling.rs`** → keep **both** our
  `toggle_selection_tabbed`/`cycle_column_width_preset` and upstream's
  `contains_layout`.

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

### Formatting (use nightly rustfmt — important)

`rustfmt.toml` turns on **unstable** options (`unstable_features = true`,
`overflow_delimited_expr`, `brace_style = 'PreferSameLine'`, `fn_single_line`,
`where_single_line`, ...). Those only apply on **nightly** rustfmt. **Stable**
rustfmt parses the file, warns *"unstable features are only available in nightly
channel"*, and **silently ignores them** — so a stable `cargo fmt` reflows the
*entire* tree to stable defaults, i.e. a massive spurious cross-file diff.

To make the right thing the default, the devShell toolchain pins rustfmt to
nightly (`latest.rustfmt`, see `nix/package.nix`). So **inside `nix develop` /
direnv**:

```bash
just fmt          # cargo fmt --all (nightly) — the only correct way
just fmt-check    # cargo fmt --all --check (matches CI)
```

CI enforces the same via `dtolnay/rust-toolchain@nightly` + `cargo +nightly fmt
--all --check` (`.github/workflows/rust.yml`).

**Never** run a plain-shell `cargo fmt` — that resolves to system *stable*
rustfmt and will churn every file. This bit us once: a stable `cargo fmt` (run to
tidy imports) got swept into the hint-bar commit and reformatted ~40 unrelated
files. The fix was to reformat with real nightly rustfmt (via
`nix shell 'github:nix-community/fenix#latest.rustfmt'`) and restore the files we
never touched: `git checkout <parent> -- <untouched files>`, then
`git commit --amend`. Keep feature commits to the files the feature actually
changes.

Note: the baseline isn't guaranteed clean under the *latest* nightly (CI's
`@nightly` floats; the flake pins one via `flake.lock`), so `fmt-check` may flag
files you didn't touch. That drift is pre-existing — do **not** reformat them, or
you recreate the churn.

### Benchmarking a perf change (counter-based A/B)

We caught a real CPU hog this way: the hint bar re-rendered on *every* layout
apply, and one focus move fans out to ~16 applies inside the WM — so 100 focus
moves = ~1,649 bar renders. The fix (skip-unchanged render + debounce + reactor
dedup) cut that ~15×. This is the repeatable method for any "is it actually
faster?" question — counting discrete events beats eyeballing `%CPU`.

**1. Instrument with counters.** Emit one `trace!` per event on a dedicated
target, so you can count from the log with zero noise at normal levels:

- `src/actor/reactor/managers.rs` — `trace!(target: "hbbench", "hb_sent")` per
  snapshot the reactor pushes to the bar.
- `src/actor/hints_bar.rs` — `trace!(target: "hbbench", "hb_render")` per actual
  render.

They are `trace!`, hence silent by default. Enable by adding `hbbench=trace` to
`RUST_LOG` (the nix module `services.rift.logLevel`, or the launch plist env)
then `just restart`. (While actively benchmarking it's fine to bump them to
`info!` temporarily so they show under the default `info` level.)

**2. A/B the two versions.** These three optimizations are now *unconditional* —
we removed the `coalesce` toggle once it was validated (they're strictly better;
see the numbers below). To compare a *future* change, either temporarily gate it
behind a throwaway hot-reloadable config flag (flip it + re-run the same workload,
no rebuild — what we did here), or build two git revisions and run the identical
workload against each.

**3. Run identical workloads, diff the counts.** Drive deterministic load with
`rift-cli` (each command triggers one layout apply) and count with `grep -c`:

```bash
CLI=./target/release-fast/rift-cli            # or ./result/bin/rift-cli
count() { grep -c "$1" /tmp/rift.err.log; }
bench() { local l="$1"; shift; local r0 s0
  r0=$(count hb_render); s0=$(count hb_sent)
  "$@"; sleep 0.7
  echo "$l: sent=$(( $(count hb_sent)-s0 )) render=$(( $(count hb_render)-r0 ))"; }

# redundant applies (scroll clamps to the same spot) + genuine changes (focus)
redundant() { for _ in $(seq 100); do $CLI execute layout scroll-strip 999 >/dev/null; done; }
focus()     { for _ in $(seq 50);  do $CLI execute window focus left >/dev/null
                                       $CLI execute window focus right >/dev/null; done; }

bench "redundant" redundant
bench "focus"     focus
# then swap in the other build (or flip the temp flag) and re-run the same two
```

Results we measured (sent / render):

| workload       | naive       | optimized |
|----------------|-------------|-----------|
| idle (3 s)     | 1 / 2       | 0 / 0     |
| redundant ×100 | 106 / 106   | 1 / 1     |
| focus ×100     | 1649 / 1649 | 146 / 107 |

The focus row is the tell: the naive bar rendered on all ~1,649 fan-out applies;
the fix collapses it via reactor dedup (skip identical snapshots) + a 12 ms
debounce (coalesce bursts) + render-gating (skip unchanged, so idle/redundant →
~0).

**Generalize it:** for any subsystem — (a) add `trace!(target: "<name>bench", …)`
counters at the hot points, (b) put the optimization behind a hot-reloadable
config flag, (c) A/B the same `rift-cli`-driven workload and diff the counts. One
build covers both sides. For a wall-clock cross-check, sample the process instead:
`ps -o cputime= -p "$(pgrep -f Rift.app/Contents/MacOS/rift)"` before/after a
fixed workload and diff — lower CPU-time for the same work = win.

---

## 7. Quick reference

```bash
# how far are we from upstream?
git fetch upstream
git rev-list --left-right --count upstream/main...xieyt/4
#  -> "<N behind>  <M ahead>"   ; M = number of commits we carry on top

# what exactly do we carry on top of upstream?
git log --oneline upstream/main..xieyt/4
git diff --stat upstream/main..xieyt/4
```

---

## 8. Inspiration & roadmap (ideas, not commitments)

Rift already covers most of niri's headline features — overview with live
thumbnails (`ui/mission_control.rs`), a scrollable layout (`systems/scrolling.rs`),
window rules (`AppWorkspaceRule`), trackpad gestures (`gesture_tap.rs`), named
per-workspace layouts. So this is the *gaps* worth stealing, ranked.

### Features to port (feasible on a macOS AX/SkyLight WM)
1. **Tabbed column display** *(shipped — §2.C tree/stack titles, §2.D niri scrolling columns)*. Rift's stack layout already
   collapses a group to one frame and draws a segmented, click-to-focus indicator
   (`ui/stack_line.rs`); the "tabs" work adds window-title labels + a tab-style
   fill on that existing indicator. Pure display variant, no new layout plumbing.
2. **Per-column width presets + cycling** — scrolling has a single uniform
   `column_width_ratio`; niri's ergonomics come from `switch-preset-column-width`
   (cycle 33/50/66%) and independent per-column widths. Pure layout math in
   `scrolling.rs` + a couple of `LayoutCommand`s.
3. **Richer window-rule *open* actions** — `open_floating`, `open_maximized`,
   `default_column_width`. *(Partly upstream as of `6c64d8b` "more app rule
   fields": `floating` now has real adoption-time effect via
   `AppRulePlacement::resolve_frame`, and tiled `size.w` covers
   `default_column_width` in **pixels, not a ratio**; `open_maximized` is still
   missing. `95bc739`'s `BaseLayoutSettings::resolved_base_for(mode)` is the
   override-resolution seam to extend.)*
4. **Workspace reordering / on-the-fly named workspaces** — niri's
   move-workspace-up/down. We have create/switch/move-to; reordering is missing.
5. **Interactive mouse resize of tiles** — we have drag-*swap*
   (`collect_drag_swap_candidates`), not drag-*resize*.

**Deliberately skipped (compositor-only, fights macOS):** VRR, direct scanout,
damage-tracked rendering, block-out-from-screencast. macOS owns the compositor.

### Stability roadmap (higher priority than features)
1. **CI-runnable tests.** The `move_focus = "left"` config panic (§2.B) shipped
   because nothing *ran* the tests. **Write `default_config_parses()`** — verified
   2026-08-05, it does not exist; `Config::default()` (which `.unwrap()`s the
   embedded `rift.default.toml`) is covered only incidentally via
   `reactor/testing.rs`, and since a panic aborts the whole binary a bad default
   config kills the run with no attributable failure. Then fix the unwinder
   (`failed to initiate panic` → a panic aborts the whole run instead of
   reporting `FAILED`, so one bad test hides the rest), and seam the window
   server behind the existing `testing.rs` mock so reactor/layout tests run
   headless. Then wire `just test` into `nix flake check`.
2. **Panic hygiene in runtime paths.** A WM should degrade, not crash. Audit
   `.unwrap()`/`.expect()` on fallible paths (`Config::default`, `dirs::home_dir`,
   AX calls). `reload_config` must validate-and-keep-old on a bad edit.
3. **AX/SkyLight race hardening.** The recurring hang/crash class (raise
   timeouts, focus-follows-mouse, sleep/display-churn, floating ping-pong).
   `raise_manager` already has good deterministic tests — extend that pattern to
   the churn/timeout/cancel paths.
4. **Property-test the layout math.** bsp constraints, scrolling offsets, master
   ratio are pure functions — proptest invariants (no overlaps, widths sum to the
   tiling area, focus always resolvable) catch geometry regressions cheaply.
