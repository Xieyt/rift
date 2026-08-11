# Fork maintenance guide

This is a **fork** of [`acsandmann/rift`](https://github.com/acsandmann/rift) that
carries a small set of local changes on top of upstream. This document explains
what we add, why, and — most importantly — how to keep pulling upstream updates
without pain.

Read this before syncing with upstream or adding new local features.

> **Syncing with upstream?** `UPSTREAM-SYNC.md` is the record of the 2026-08
> merge (38 commits, five checkpoints, now 0 behind): per-commit verdicts, the
> resolutions that landed, and — most useful — §4.6 "Predictions that were wrong".

---

## 1. Repository topology

| Remote | URL | Role |
|---|---|---|
| `origin` | `git@github.com:xieyt/rift.git` | our fork (push here) |
| `upstream` | `git@github.com:acsandmann/rift.git` | source of truth (never push here) |

Branches:

- **`xieyt/5`** — the working branch: `upstream/main` **merged** with our local
  changes on top. This is what we build and ship. (Older notes called it
  `udpate/to-latest`, then `xieyt/4`; the live branch is `xieyt/5`.)
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
`.github/workflows/release.yml`, `.gitignore`, `assets/Info.plist`, `Cargo.toml`
(adds an explicit `[[bin]] rift-cli`).

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
- **ScreenCaptureKit SDK pin.** Upstream's Mission Control runs on
  ScreenCaptureKit, which needs the macOS **12.3+** SDK. The shared crane `args`
  pin it explicitly — `buildInputs = [ pkgs.apple-sdk_15 (pkgs.darwinMinVersionHook "12.3") ]`
  — instead of inheriting whatever `clangStdenv` happens to default to, so a
  nixpkgs bump can't silently break the `objc2` framework link. No explicit
  `-framework` flags are needed (the `objc2-*` crates emit
  `#[link(kind = "framework")]`), and both are `buildInputs`, so `strictDeps = true`
  stays valid. The devShell needs the SDK **and** an explicit
  `DEVELOPER_DIR = "${pkgs.apple-sdk_15}"`: it uses a naked stdenv, so **no setup
  hook fires** — neither apple-sdk's `DEVELOPER_DIR` hook nor
  `darwinMinVersionHook` applies — and the cc/bintools wrappers read
  `DEVELOPER_DIR` at runtime, so without it an ambient `cargo build` cannot find
  ScreenCaptureKit. Deliberately **no** `MACOSX_DEPLOYMENT_TARGET` there:
  `darwinMinVersionHook` is a *floor*, the ambient default already sits above 12.3,
  and pinning it would diverge from the crane build.
- **Workspace scoping — placement is the whole trick.** Upstream is now a cargo
  workspace: the root `rift-wm` package plus two upstream crates,
  `crates/rift-protocol` and `crates/rift-client`.
  `cargoExtraArgs = "--locked --package rift-wm --bins";` **must live in the shared
  `args` attrset**, not in the `buildPackage` call. crane keys `cargoArtifacts`
  reuse on the argument set, so if `buildDepsOnly args` and
  `buildPackage (args // { … })` disagree on `cargoExtraArgs`, every dependency
  **silently rebuilds** — slow, not an error, and easy to miss for months. It also
  keeps the `rift-client` examples' `ctrlc`/`nix`/`dispatch2` dev-dependency subtree
  out of the build. `src` filtering and `crateNameFromCargoToml` needed no change:
  `commonCargoSources` is recursive over `crates/**`, and the root kept
  `[package] rift-wm` beside `[workspace]` (a virtual manifest would have broken
  `pname`).
- **Screen Recording consent — a new TCC class.** Both plists carry
  `NSScreenCaptureUsageDescription`: `assets/Info.plist` **and** the heredoc in
  `nix/package.nix` that is the one actually shipped. Without the purpose string
  macOS **kills the process** at the prompt instead of showing it. There is no
  entitlement for screen capture — it is purely TCC-gated
  (`kTCCServiceScreenCapture`), so no `.entitlements` file and App Sandbox stays
  off. Screen Recording is a **separate consent record** from Accessibility, keyed
  on the same identity + bundle-id: with the stable `rift-codesign` identity the
  grant survives `darwin-rebuild` exactly like Accessibility, but **every existing
  user gets one new prompt at the next login**, raised by a launchd agent, which is
  easy to miss. Ad-hoc (`signingIdentity = "-"`) installs get re-prompted on
  **every** rebuild, same as Accessibility. See §2.F for what happens when the
  grant is denied.

**Why:** upstream ships no Nix support at all.
**Conflict risk:** LOW, up from ~zero. Still almost entirely *new files* upstream
doesn't have — but `Cargo.toml` is now a **workspace root**, so upstream touches it
whenever it adds a crate or a dependency, and the SDK pin plus `cargoExtraArgs`
scoping must be re-checked whenever upstream adopts a new framework or crate.
`nix build .#rift-unwrapped` is the cheap proof that the scoping still holds.

### B. `feat: app activation on focus`
**Files:** `src/actor/app.rs`, `src/actor/raise_manager.rs`,
`src/actor/reactor.rs`, `src/layout_engine/engine.rs`, `src/bin/rift-cli.rs`,
`src/sys/app.rs`, `src/actor/reactor/events/{command,window}.rs`,
`src/common/config.rs`, **`crates/rift-protocol/src/commands.rs`**.

- Threads an `activate: bool` flag through the entire raise pipeline:
  `Request::Raise` / `RaiseRequest` (app.rs) → `raise_manager` → `reactor`
  → `LayoutCommand::MoveFocus(MoveFocusArgs { direction, activate })` → `EventResponse.activate`.
- Adds `NSRunningApplication::activate()` (`sys/app.rs`) and the
  `rift-cli execute window focus <dir> --activate` CLI flag.
- **Single policy:** `[settings.layout] activate_on_focus` (default `true`) gates
  *both* keyboard focus moves and workspace switches; the per-command `activate`
  flag overrides it upward. Set it `false` to require the explicit flag. Upstream's
  app-rule `focus = true` path (`6c64d8b`) also carries
  `activate: activate_on_focus`, so a rule-driven focus grant obeys the same policy
  as every other focus path.
- **`MoveFocusArgs` lives in `crates/rift-protocol/src/commands.rs`**, not in
  `src/` — upstream's `e546861` moved `LayoutCommand` out of the binary crate, so
  our variant had to follow. `layout_engine/engine.rs` re-exports it
  (`pub use rift_protocol::{LayoutCommand, MoveFocusArgs};`), which is why every
  `src/` callsite still compiles unchanged. Consequence: **our `activate` flag is
  now part of the public wire vocabulary and we own its compatibility.**
- **Config back-compat (CRITICAL):** `MoveFocus` carries `activate`, but it
  deserializes through a custom `MoveFocusArgs` impl (now in
  `crates/rift-protocol/src/commands.rs`) that accepts *both* the bare
  `move_focus = "left"` (upstream's default-config form; `activate` defaults
  `false`, `Repr::Bare`) *and* the table
  `move_focus = { direction = "left", activate = true }` (`Repr::Full`). Without the
  dual-form parse, `Config::default()` — which parses the embedded
  `rift.default.toml` — panics, so app startup and every test that uses defaults
  abort. `cargo check` does **not** catch this (it never runs `Config::default()`);
  `just test` does, now by name: `common::config::default_config_parses` asserts the
  four bare `move_focus` binds still resolve. If you touch `MoveFocus`, keep both
  forms parseable.
  That same impl is now **also the legacy-IPC compatibility path**: upstream's
  `103af15` shim routes old untyped `{"Reactor":{"move_focus":"left"}}` payloads
  through this exact `Deserialize`. One impl, two back-compat duties — default-config
  parsing and every stale `rift-cli` / sketchybar / Lua consumer.
- **Activation attribution (CRITICAL, runtime-only).** Upstream `652a53f` makes the
  Carbon front-process edge authoritative: `handle_application_activated` treats
  `Quiet::Yes` as "initiated by Rift" and *skips* the auto workspace switch, while
  `Quiet::No` means "the user did this" and absorbs one. Our
  `ActivateIgnoringOtherApps` synthesises exactly such an edge, and it reaches
  `on_global_activation` *before* `wait_for_activation` arms `last_activated` — so
  unattributed it resolves to `Quiet::No` and fires a **spurious auto workspace
  switch**. Two fixes, both mandatory:
  1. `src/actor/app.rs` arms
     `pending_activation_quiet = Some((Instant::now(), Quiet::Yes))` immediately
     **before** `let _ = this.running_app.activate()`. The marker is consumed within
     a 1 s window (upstream's own `on_ax_activation_changed` idiom) and cleared by
     `handle_frontmost_changed` when the app stops being frontmost. **Order is the
     fix: arm, then activate.**
  2. `src/actor/reactor.rs` clears `response.activate` on the
     `WorkspaceSwitchOrigin::Auto` path — that switch *reacts* to an app already
     becoming frontmost, so re-foregrounding it is a wasted
     `ActivateIgnoringOtherApps` that can feed back into `on_global_activation`.
     Keyboard and IPC switches keep their `activate`.

  **No test covers either fix.** `cargo check` and `just test` both pass with them
  reverted, so this needs a manual check after any merge touching `app.rs`
  activation or `WorkspaceSwitchOrigin`.

  **Repeatable procedure (executed 2026-08-08, passed).** Reproduce the exact
  hazard — an app that is frontmost-able on the active workspace *and* owns a window
  on another one, which is what makes a misattributed edge jump workspaces:

  ```bash
  CLI=./result/bin/rift-cli
  ws() { $CLI query workspaces | python3 -c \
    "import json,sys;print([w['index'] for w in json.load(sys.stdin) if w['is_active']])"; }
  # park one window of a multi-window app on another workspace, stay put
  $CLI execute workspace move-window 0      # no --follow
  ws                                        # must still print the original workspace
  # now hammer the activation path; the workspace must never change
  for i in $(seq 6); do
    $CLI execute window focus left --activate  >/dev/null; sleep 0.8; ws
    $CLI execute window focus right --activate >/dev/null; sleep 0.8; ws
  done
  ```

  Result: Emacs parked on ws0 while still present on ws3, twelve `--activate`
  moves, active workspace stayed `3` every time — no spurious switch. Also confirm
  `--activate` actually foregrounds (`osascript -e 'tell application "System Events"
  to get name of first process whose frontmost is true'` changes with `--activate`
  and not without it) and that the log shows
  `MoveFocus(MoveFocusArgs { direction: …, activate: true })`.

  **What the log will show, and why it is not a bug:** the resulting
  `ApplicationActivated(pid, …)` is usually `Quiet::No`, not `Quiet::Yes`. That is
  upstream's intended attribution for an *explicit* focus command —
  `on_global_activation` prefers `last_activated` (armed by `wait_for_activation`
  with the raise's own `quiet`) over our `pending_activation_quiet` marker. Our
  pre-arm covers the narrower path where `last_activated` is *not* armed
  (`waits_for_activation == false`, e.g. the app is already frontmost), which is
  exactly the case that would otherwise resolve to a bare `Quiet::No` with no raise
  in flight. Do not "fix" the `Quiet::No` you see on a keyboard focus move.

**Effect:** a focus move / workspace switch brings the target *app* to the
foreground, not just shifting keyboard focus.
**Why:** upstream only moves keyboard focus; it never foregrounds the app, and is
not moving that way. Re-verified against the merged upstream tip (`6c64d8b`): zero
occurrences of `NSRunningApplication::activate` / `activateWithOptions` in
upstream's `src/actor/app.rs` or `src/sys/app.rs`, and upstream's `RaiseRequest` is
still the 4-tuple `(wids, token, sequence_id, quiet)` against our 5-tuple. The one
`activate()` upstream added is `NSApplication::sharedApplication(mtm)` in the
Mission Control overlay — Rift activating *itself* to take key focus, not per-app
foregrounding. **This stays fork-only and is not even partially duplicated.**
**Note:** `sys/app.rs` intentionally uses the deprecated
`activateWithOptions(ActivateIgnoringOtherApps)` under `#[allow(deprecated)]` — it
gives the forceful foregrounding a WM wants, and we target current macOS, so the
deprecation is irrelevant here.
**Conflict risk:** HIGH — touches ~7 core files upstream changes constantly, and
now spans a crate boundary as well. Kept as a *single* commit so you resolve it
once per upstream bump, not once per sub-change.

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
- **Horizontal move splits on `tabbed` (BEHAVIOUR CHANGE, deliberate).** Upstream
  makes a horizontal `move_node` on a multi-window column *extract* the focused
  window into its own neighbour column — "the fast way to undo an accidental
  stack". Our `5e5916f` did the opposite for every column. Resolved during the
  2026-08 merge by splitting on `Column.tabbed`:
  - **tabbed → still moves as a whole.** Ours (`dcc768d`), kept: a tab group *is*
    one unit; use `consume-or-expel-window` to move windows in or out. Test
    `horizontal_move_shifts_tabbed_column_as_a_whole`.
  - **plain vertical stack → extracts.** Upstream's, adopted. Extraction also
    clears the source column's `active` tab memory, so it cannot point at a window
    that no longer lives in that column. Tests
    `horizontal_move_extracts_selected_window_from_a_non_tabbed_stack` and
    `move_selection_{right,left}_extracts_from_a_lone_stacked_column` (which
    replaced the three `..._as_a_whole` / `..._does_not_extract_...` assertions).

  `5e5916f`'s behaviour is **dropped** — see §2's dropped list for why we did not
  defend it.
- **Upstream fields on `Column`.** `Column` now also carries upstream's
  `width_overridden: bool` (`6c64d8b`) beside our `tabbed` and `active`. All three
  show up in the 21 exhaustive `Column { .. }` literals in `scrolling.rs`, 12 of
  which are fork-only test code — see §4.
- **Moving a column now reveals it (upstream `1de4d09`, #437).** After
  `move_selection`, upstream reveals the moved column via
  `reveal_selected_without_direction()` instead of `align_scroll_to_selected()`
  **when `focus_navigation_style = "niri"`** — previously a `move_node` could scroll
  the column you just moved off-screen. It lands in `move_selection`, one level
  *above* our `move_selected_window_horizontal` tabbed split, so the two are
  independent; the merge was conflict-free. Worth knowing when reading the split:
  the visible screen x of the focused column often stays *constant* across
  successive `move_node`s (it is re-revealed at the same spot each time), so verify
  a move by checking column **order**, not coordinates. Verified live 2026-08-08:
  order went 3 → 2 → 1 across two `move-node left`s while staying on-screen.

**Why:** the *real* niri tabbed-columns feature — §C only decorated the tree/stack
indicator, which the scrolling layout never emits.
**Conflict risk:** MEDIUM–HIGH — touches `scrolling.rs`, upstream's most-churned
file. Kept minimal: one `Column` field, one `calculate_layout` branch, one
indicator method, one trait default, one engine arm + command.
**Verify:** `just test` (`tabbed_column_overlaps_windows_and_emits_one_group`,
`toggle_selection_tabbed_flips_the_column_flag`,
`horizontal_move_shifts_tabbed_column_as_a_whole` plus its extract counterpart
`horizontal_move_extracts_selected_window_from_a_non_tabbed_stack`). Visuals need
`just install`.

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
  `update_layout`). Upstream's `space_scope: Option<SpaceId>` parameter (`fe97ae2`,
  on `update_layout`/`calculate_layout`/`update_layout_or_warn`) must be threaded
  through that recursion **unchanged** — hand it `None` and a scoped workspace
  switch silently widens to a full re-layout every time the bar re-reserves.
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
- **Hot-reload arm is a permanent conflict site.** The reactor's outcome fan-out
  (`reactor.rs`, `if let Some(config) = outcome.service_config_update`) carries our
  `hints_bar_tx → hints_bar::Event::ConfigUpdated(config.clone())` arm beside
  upstream's `stack_line_tx` / `menu_tx` / `wm_sender` arms. Upstream's `b619ce5`
  **rewrote that exact block** (`Option<(Config, bool)>` → `Option<Config>`), and a
  careless "take theirs" **silently deletes hints-bar hot reload**: nothing fails to
  compile, and no test covers it. Re-add the arm every time upstream touches that
  block. Standing playbook item — see §4.

**Why:** a niri-style "where am I in the strip + what's in each column" HUD; it
also supersedes the stack-line tab bar for scrolling (turn `stack_line` off).
**Conflict risk:** LOW for the new files; the *touched* core files are where it
bites — see §4 items 7 (the `hints_bar_tx` hot-reload arm) and 3, plus the
`space_scope` threading above.
**Verify:** `just test` now covers the six `ui::hints_bar` geometry/click tests —
its allowlist gained the `ui::hints_bar` filter on 2026-08-08, having silently
omitted it since the feature landed (see §6 "Tests"). Visuals need `just install`
on a real Mac (SkyLight windows can't render headless); verified there 2026-08-08 —
workspace rail, hint letters, focused chip fill, `show_titles`, the detail overlay,
and the `overflow = "bar"` second strip all render, and `focus-column N` lights the
Nth `keys` letter (`focus-column 2` → `D` for `keys = "asdfghjkl;"`).

### F. `fix: degrade Mission Control previews when Screen Recording is denied`
**Files:** `src/ui/mission_control.rs`.

- Upstream's Mission Control runs on ScreenCaptureKit, which is TCC-gated
  (`kTCCServiceScreenCapture`, §2.A). A denied grant gives a silently blank preview
  grid with no explanation. We probe `CGPreflightScreenCaptureAccess()` **once**
  through a `LazyLock<bool>` (`SCREEN_CAPTURE_PERMITTED`) and fail the capture batch
  with one clear `warn!` naming the missing permission instead of rendering nothing.
  Preflight only — it never raises the prompt, so it is safe to call on any capture
  path.
- **Mission Control is OPT-IN, and that is the first thing to check.**
  `rift.default.toml` ships `[settings.ui.mission_control] enabled = false` under a
  `# experimental mission control` comment, and `MissionControlActor::run()` gates on
  it *inside* its receive loop:

  ```rust
  while let Some((span, event)) = self.rx.recv().await {
      if self.config.settings.ui.mission_control.enabled {
          self.handle_event(event);   // dropped entirely when disabled
      }
  }
  ```

  So with the default config, `rift-cli execute mission-control show-all` returns
  `"Command executed successfully"`, the reactor logs the command, `wm_controller`
  forwards it — **and nothing happens, silently, with no log line from the actor.**
  That is indistinguishable from a broken feature, and it cost a full debugging
  detour during the 2026-08 sync: the symptom was read as an upstream regression in
  `0bf5549` before anyone checked the flag. Verified working once enabled: live
  ScreenCaptureKit previews, legible window contents, focus border, dimmed backdrop.

  **Triage order for "Mission Control does nothing / is blank":**
  1. `[settings.ui.mission_control] enabled = true` in your config? (Not in
     `dev-config.toml` by default.) This is almost always it.
  2. Screen Recording granted? Check for this section's `warn!`, or
     `sqlite3 ~/Library/Application\ Support/com.apple.TCC/TCC.db \
     "select service,auth_value from access where client='git.acsandmann.rift'"`
     — `kTCCServiceScreenCapture` with `auth_value = 2` means granted.
  3. Only then suspect the code.

**Why:** a WM should degrade loudly, not silently (§8 stability item 2). The new
Screen Recording prompt is easy to miss (§2.A), so "Mission Control is blank" would
otherwise arrive as an unattributable bug report.
**Conflict risk:** LOW — one `LazyLock` plus one guard at the top of `capture`, in a
file upstream owns but away from its SCK plumbing.

### Dropped / superseded (do NOT re-add)
- **scrolling off-left snap** — a per-column `x` clamp in `scrolling.rs`. Reverted:
  upstream already clamps the scroll offset, so it was a redundant heuristic in
  upstream's most-churned file. Reverted to match upstream exactly.
- **`5e5916f` "horizontal move shifts non-tabbed stacks as a whole"** — dropped in
  the 2026-08 merge. Upstream has a deliberate, defensible **opposite** behaviour
  (extract the focused window into its own neighbour column), and it lives in
  `scrolling.rs`, upstream's most-churned file — so carrying a bare *preference*
  there buys a conflict every single sync for no functional gain. Extraction is also
  the more useful default: it is the fast way to undo an accidental stack, and
  `toggle_column_tabbed` recovers whole-column movement the moment you actually want
  it. `dcc768d`'s tabbed-column behaviour is kept (§2.D) — that one is a semantic
  claim about what a tab group *is*, not a preference.
- **the `maybe_send_menu_update()` call in `managers.rs`** — deleted, superseded by
  upstream `fe8a687` "remove duplicate menu update", which relocated the call into
  `reactor.rs`. **Correction to `UPSTREAM-SYNC.md` §3's `DROP-OURS` label:** that
  line was **upstream's**, not a fork addition (verified: `fe8a687^`'s
  `managers.rs` has it, `fe8a687`'s does not). It only looked like ours because our
  local edits sit next to it — which is exactly how inherited code gets mistaken for
  a feature during a merge. The function itself stays in `query.rs` with its two
  legitimate `reactor.rs` callers; do not re-add the per-layout-apply call.
- **fallback-focus rename**, **layout-focus-on-activation**, and a dead
  `raise_window` helper — all superseded by upstream's declarative `EventOutcome`
  model (`outcome.focused_window`). See §5.

---

## 3. Syncing with upstream (rebase vs merge)

**Use `merge`, not `rebase`, for routine syncs.** We carry a large feature set (see
§2) and sync periodically, so upstream deltas are non-trivial. A merge resolves the
overlap **once** per sync, keeps commit hashes stable (no force-push, safe for a
shared branch), and — with `git rerere` — replays a resolution you've already done.
Rebase only wins if you sync *very* frequently (tiny deltas) or want a linear
history to open an upstream PR; then force-pushing the fork branch is expected. That
is **not** our normal flow — rebasing our stack over a real upstream delta is a
per-commit conflict marathon; merging is one pass.

**Merge in chronological checkpoints, not one jump.** The lesson of the 2026-08 sync
(38 commits): pick checkpoint SHAs along `upstream/main` and `git merge <sha>` each
in order, gating every one on `cargo check --workspace --all-targets` + `just test`
before finalizing its merge commit. Why that beats a single `git merge upstream/main`:

- Upstream commits depend on each other, so per-*cluster* cherry-picking is wrong;
  chronological checkpoints respect every dependency by construction.
- Each stage's test delta is attributable to a handful of upstream commits instead
  of all 38, and `git bisect` stays meaningful.
- Cheap stages land immediately: in 2026-08 the first 13 commits merged with **zero
  conflicts**, and two thirds of all hunks came from just three structural commits.
- Each stage's resolutions land in `rerere` separately.

Cost: N merge commits instead of 1 — irrelevant, we never rebase this branch. See
`UPSTREAM-SYNC.md` §6 for the worked five-stage example and §7 for *when* to sync
(on structural change, not on a commit count).

`git rerere` is enabled (`git config rerere.enabled true`) so each conflict you
resolve is recorded and auto-applied next time. Do **not** count on it across a
large delta: verified 2026-08-05, **none** of the 3 resolutions then in
`.git/rr-cache` matched any of the 16 hunks in the 38-commit backlog — upstream had
moved the surrounding code, a 0% hit rate. That sync recorded 12 fresh resolutions,
but `.git/rr-cache` is **local-only and uncommitted**: it protects one clone and
nothing else. rerere pays off for repeated *small* syncs; §4 below is the durable
store.

**Routine (merge) — use the recipes; they encode this procedure:**

```bash
just upstream-status   # drift + structural tripwires: does this need syncing NOW?
just sync              # one range merge, gated on build + tests
git push origin xieyt/5
```

`just sync` merges the **whole range at once** (`sync-to upstream/main`), then gates
on `cargo check --workspace --all-targets` and `just test`. It refuses to start on a
dirty tree.

**Why a range merge and not commit-by-commit** — this is counter-intuitive, and the
first version of the recipe got it wrong. A range merge is the *smaller* conflict
set, because git's 3-way merge over the range absorbs upstream's internal churn,
including commits it later reverts. Measured on `b31dddf..7829780`:

| | conflicted files |
|---|---|
| range merge (what the recipe does) | **2**, both additive |
| commit-by-commit | **7** at `1b69d8e` alone |

`1b69d8e` rewrites `Request::Raise` to carry `FocusConfirmation` exactly where our
fork carries `activate: bool` (§2.B) — so stepping commit-by-commit makes you
resolve the fork's most delicate feature across 7 files, and then `7829780` reverts
the lot. Same destination, all the risk.

Still too big to resolve in one bite? Pick a checkpoint SHA: `just sync-to <sha>`
(UPSTREAM-SYNC.md §6 split the 38-commit backlog into 5 hand-picked checkpoints).
Choose boundaries that do **not** split a revert pair, or you buy the problem above.

> Two corrections, both from 2026-08-10. Until that day `just sync` did `git rebase
> upstream/main` — the strategy this section explicitly rejects — plus an unannounced
> `git push`. Its replacement then merged commit-by-commit, which is the trap
> described above. If you remember either, the memory is stale.

The manual equivalent, when you want to drive a single checkpoint by hand:

```bash
git fetch upstream
git push origin upstream/main:main      # keep the clean mirror in sync (fast-forward)
git checkout xieyt/5
git log --oneline HEAD..upstream/main   # more than a handful? pick checkpoints
git merge <checkpoint-sha>
# ...resolve conflicts if any (see §4), then:
git add -A
cargo check --workspace --all-targets   # --lib misses the test targets AND crates/
just test                              # our curated suite must pass
git commit                              # finalize this stage's merge commit
```

When a test goes red during a sync, establish *whose* bug it is before debugging it:
`just upstream-test <TEST>` runs it on a detached pristine `upstream/main` worktree.
Upstream has shipped a red test at its own tip at least once (§8.3 of
`UPSTREAM-SYNC.md`).

**Rebase alternative** (only for upstreaming / a linear history):

```bash
git checkout xieyt/5
git rebase upstream/main             # resolve per-commit; rerere still helps
git push --force-with-lease origin xieyt/5
```

---

## 4. Conflict-resolution playbook

Conflicts land in the files our churny features touch. Historically that is
**feature B**'s `activate` threading and the **layout engine**; the 2026-08 sync
added a third axis — the command enums now live in a separate crate.

Resolution principles:

1. **Take upstream's structure, re-apply our layer.** When upstream renames
   variables, changes a function signature, or moves code, adopt *their* version and
   re-add our field / argument on top.
2. **`EventResponse` literals need BOTH `changed` (upstream) and `activate` (ours).**
   `b9b79be` added `changed: bool`; our `activate: bool` was already there. Missing
   either → `E0063`; destructures in `reactor.rs` need `changed: _`. Note *where* the
   work actually is: git auto-merges literals upstream also edited, so the ones that
   bite are **fork-only** handlers. In 2026-08 that was exactly one — our
   `FocusColumn` arm, which needed a hand-written `changed: true`.
3. **`Column` literals need `width_overridden` (upstream) AND our `tabbed` + `active`.**
   There are **21** exhaustive `Column { .. }` literals in
   `src/layout_engine/systems/scrolling.rs`, **12 of them fork-only test code** — no
   upstream-side edit to auto-merge, so every upstream `Column` field costs a hand
   sweep. Budget for it.
4. **The command enums live in `crates/rift-protocol/src/commands.rs`, not `src/`.**
   Our five variants belong *there*: `LayoutCommand::{MoveFocus(MoveFocusArgs),
   FocusColumn, ToggleColumnTabbed, CycleColumnWidth}` and
   `ReactorCommand::ToggleHintsBar`. Match arms, clap definitions and the `WmCmd`
   bridge stay in `src/`. If upstream moves an enum again, follow it — do not keep a
   shadow copy in `src/`.
5. **`MoveFocusArgs` must keep its `Repr::Bare` arm**, or `Config::default()` panics
   at startup and every default-using test aborts. `cargo check` **cannot** see this;
   `just test` can, by name (`common::config::default_config_parses`). Upstream's
   `MoveFocus(Direction)` newtype carries `#[serde(rename = "direction")]` in exactly
   the slot our struct occupies, so the conflict recurs on every protocol change. The
   same impl is also the legacy-IPC decode path (§2.B) — do not simplify it.
6. **Keep our non-panicking `home_dir()` in `src/common/config.rs`.** Upstream still
   writes `dirs::home_dir().unwrap()` (three sites at the 2026-08 tip). Ours sits on
   the *our* side of the hunk, so "take theirs" reintroduces a startup panic. §8
   item 2.
7. **Preserve the `hints_bar_tx` arm in the reactor's outcome fan-out**
   (`if let Some(config) = outcome.service_config_update` in `reactor.rs`). Upstream
   rewrites that block — `b619ce5` did — and dropping our arm compiles fine, passes
   every test, and silently kills hints-bar hot reload. §2.E.
8. **When git interleaves two test tails, reconstruct from both blobs — never do
   marker surgery.** It happened twice in `scrolling.rs` during 2026-08: our test tail
   and upstream's new tests share identical add/select preamble lines, so git splices
   them into nonsense that *looks* resolvable. Recover with `git show HEAD:<path>` and
   `git show <theirs>:<path>`, rebuild the region deliberately (ours, then theirs),
   and count the `#[test]` fns before and after.
9. **`RaiseRequest` literals need `activate`** too (in `reactor/events/command.rs`,
   `reactor/events/window.rs`, and anywhere upstream constructs a raise). Upstream's
   is a 4-tuple, ours a 5-tuple.
10. **`move_focus_internal` takes `window_store` as its first arg** upstream — keep
    that; our change is only the `activate` handling around the call.
11. **`make_key_result` is `Option<Result<..>>`** — use
    `.as_ref().is_some_and(Result::is_ok)`, never `.is_ok()` directly.
12. **When our old code duplicates something upstream now provides, drop ours** —
    but check *whose* line it is first. Upstream moved to a declarative "return
    `EventOutcome`, let the reactor apply it" pattern, so old imperative helpers of
    ours are dead weight; delete them and rely on upstream's path (layout focus sync
    flows through `outcome.focused_window`). In 2026-08 one "duplicate of ours" turned
    out to be inherited upstream code — see §2's dropped list.

After resolving: `git add -A`, then `cargo check --workspace --all-targets` and
`just test` — **merge:** `git commit`; **rebase:** `git rebase --continue`. Always
gate before committing. **Two classes of breakage pass both gates** and need a manual
`just install` smoke test: the §2.B activation attribution, and hints-bar hot reload.

### Recorded resolutions

`.git/rr-cache` holds 12 resolutions from the 2026-08 sync, but it is local-only and
uncommitted — so treat this list as the real store. Current, as landed:

- **`src/layout_engine.rs`** re-export → 3-way **union**: our `MoveFocusArgs`,
  upstream's `LayoutEventOutcome`, and the `Restore*` set
  (`RestoreReport`/`RestoreRequest`/`RestoreScope`/`RestoreSource`/`RestoreWarning`).
- **`src/layout_engine/engine.rs`** → *re-export* the protocol types
  (`pub use rift_protocol::{LayoutCommand, MoveFocusArgs};`) instead of defining
  them; keep our five match arms. Our `serialize_to_string` stays **dropped** —
  upstream owns it in `engine/persistence/storage.rs`.
- **`src/layout_engine/systems/scrolling.rs`** → keep our `toggle_selection_tabbed`
  / `cycle_column_width_preset` / `collect_group_containers_scrolling` *and*
  upstream's `contains_layout` + insertion-point plumbing; split horizontal
  `move_node` on `Column.tabbed` (§2.D); sweep every `Column` literal for
  `width_overridden`.
- **`src/actor/reactor.rs`** → keep our `hints_bar_tx` arm in the
  `service_config_update` fan-out, and keep `response.activate = false` on the
  `WorkspaceSwitchOrigin::Auto` path.
- **`src/actor/app.rs`** → keep the `pending_activation_quiet = Quiet::Yes` pre-arm
  immediately *before* `running_app.activate()`. Order **is** the fix (§2.B).
- **`src/actor/reactor/managers.rs`** → keep our hints-bar feed
  (`build_hints_bar_cells`, `hints_bar_reserve`) and thread upstream's `space_scope`
  through the convergence recursion unchanged; do **not** re-add the
  `maybe_send_menu_update()` call.
- **`src/common/config.rs`** → union our `[settings.ui.hints_bar]` /
  `external_bar_*` / `activate_on_focus` keys with upstream's `BaseLayoutSettings`
  tables (no name collisions so far); keep our `home_dir()`.
- **`src/actor/hints_bar.rs`** → `ReactorCommand::FocusWindow.window_id` is
  `rift_protocol::WindowId` now; both chip-click routes need `.into()`.

---

## 5. Maintenance philosophy (why the history looks like this)

Rebase pain scales with **(core-file lines we touch) × (commits spreading those
touches)**. So:

- **One commit per feature, never many.** A feature split across N commits editing
  the same lines makes you resolve the same conflict N times. Squash before you
  push a new feature.
- **Quarantine packaging from behavior.** The Nix commit touches almost only new
  files, so it rarely conflicts — keep it separate and mentally skip it during
  merges. Its one shared file, `Cargo.toml`, is now a workspace root, so check it.
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
cargo check --workspace --all-targets   # the gate: covers crates/ AND test targets
cargo check                             # faster, narrower — fine mid-resolution
cargo build --lib                       # compiles the library
nix build                               # real end-to-end .app build
nix build .#rift-unwrapped              # raw binaries; proves the workspace scoping
```

`--workspace --all-targets` matters since upstream became a workspace: plain
`cargo check` misses `crates/rift-protocol` (where our command variants live, §4
item 4) and `--lib` misses the test targets that hold the exhaustive `Column` /
`EventResponse` literals (§4 items 2–3). Neither form can catch the
`Config::default()` serde panic — only `just test` does (§4 item 5).

### Tests

```bash
just test            # fast deterministic logic tests (our coverage); no GUI
just test-all        # whole lib suite — only in a real GUI session
```

`just test` runs through `nix develop` so it links even without direnv. It is a
curated allowlist, currently **363 tests** (327 before the 2026-08 upstream merge;
352 after it; 359 once the hint-bar filter was added; 363 after the 2026-08-10
sync). Four things to know:

- **Two known upstream failures**, both re-verified by running them on a pristine
  `upstream/main` worktree — neither is ours, don't chase them. Use
  `just upstream-test <TEST>` to re-check either one, or any future red test:
  - `topology_change_clears_stale_pending_hide_target_before_next_workspace_layout`
  - `ax_invalidation_after_quarantine_release_preserves_live_layout_state` — arrived
    with upstream's `1e3a898`; the model survives an AX-element swap but the layout
    node does not. See UPSTREAM-SYNC.md §8.3.
- **Window-server tests need a real GUI session.** Tests touching SkyLight
  (`SLS…`) abort with an objc weak-reference error in a headless/agent shell,
  so `just test` excludes them; use `just test-all` from your logged-in desktop.
  `just test-all` therefore still fails on the known upstream tests above — that is
  expected, not a regression.
- Panics in the test binary *abort* instead of unwinding (`failed to initiate
  panic`), so one failing test kills the whole run — which is why `just test` uses a
  curated allowlist rather than the full suite. This bit us three times during the
  2026-08 merge; it is the open half of §8 stability item 1. **The assertion message
  is not lost, though** — an earlier version of this note said the failure was
  unattributable, which is only half right. `--nocapture` prints the panic *before*
  the abort:
  `cargo test --lib -- <test> --test-threads=1 --nocapture`
  Without it you learn which test died; with it you learn why.
- **The allowlist hole is closed mechanically, not by promise.** Its seven module
  filters are `layout_engine`, `actor::raise_manager`, `actor::reactor::tests`,
  `common::config`, `ui::stack_line`, `ui::hints_bar`, `ui::menu_bar`. The last three
  were each missing at some point — §2.E's hint-bar tests silently never ran for
  months while §2.E claimed they did, and `ui::menu_bar` arrived with upstream's
  `abdbcc8` carrying four tests that would have gone the same way. Two doc notes
  asking the next person to remember failed twice, so the invariant is now a test:
  `common::config::tests::just_test_allowlist_classifies_every_test_module` parses
  this recipe out of the `justfile` and asserts every `#[cfg(test)] mod` in `src/` is
  either allowlisted or in its explicit `GUI_OR_DEFERRED` list. Add a test module
  without classifying it and that test fails by name.

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
**Since the ScreenCaptureKit merge there is a second grant:** Screen Recording, a
separate TCC record from Accessibility (§2.A). Expect one new prompt at the next
login after installing, and — on ad-hoc installs — on every rebuild. If Mission
Control does nothing or comes up blank, check the **`enabled` flag first** (it is
`false` by default and the actor drops events silently), *then* the grant — see
§2.F for the full triage order; §2.F also logs a `warn!` naming the permission.

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
nightly (`latest.rustfmt`, see `nix/package.nix`), **and the recipes now enforce
the dev shell themselves** — they are `nix develop -c cargo fmt …`, so they are
correct from any shell:

```bash
just fmt          # nix develop -c cargo fmt --all (nightly) — the only correct way
just fmt-check    # same, --check (matches CI)
```

CI enforces the same via `dtolnay/rust-toolchain@nightly` + `cargo +nightly fmt
--all --check` (`.github/workflows/rust.yml`).

**Never** run a plain-shell `cargo fmt` — that resolves to system *stable*
rustfmt and will churn every file. This bit us twice:

1. A stable `cargo fmt` (run to tidy imports) got swept into the hint-bar commit
   and reformatted ~40 unrelated files. The fix was to reformat with real nightly
   rustfmt and restore the files we never touched:
   `git checkout <parent> -- <untouched files>`, then `git commit --amend`.
2. **`just fmt` / `just fmt-check` / `just clippy` were themselves the trap** until
   `54ce476`. They were bare `cargo …` with only a *comment* telling the caller to
   be inside `nix develop` — unlike `just test`, which always enforced it. Outside
   the dev shell they silently used stable rustfmt, which is how mechanism #1
   happened in the first place. Fixed: all three now wrap in `nix develop -c`.

**Correction to a long-standing note here:** this section used to claim the
baseline "isn't guaranteed clean under the latest nightly" and that any drift
`fmt-check` reports is "pre-existing — do not reformat". That was **false**, and it
was load-bearing enough to hide 47 real hunks. Measured 2026-08-08 with the
flake-pinned rustfmt (1.8.0-nightly): pristine `upstream/main` is **completely
clean** (0 hunks), and our tree had 47 hunks — all in fork-owned files, all real,
now fixed. The 411 hunks the old bare recipe reported were the *stable*-rustfmt
phantom (1.8.0-stable), not baseline drift. So: if `just fmt-check` reports drift,
it is **yours** — fix it. Do not dismiss it.

**`just clippy` does not pass, and that is upstream's debt — not a gate you broke.**
Measured 2026-08-10: 8 errors, all in files byte-identical to `upstream/main` and
untouched by our merges — 4 in `src/sys/dispatch.rs` (`not_unsafe_ptr_arg_deref`), 1
in `src/actor/mission_control_observer.rs` (`never_loop`), plus their knock-ons.
Deliberately not fixed: the fixes are signature changes to upstream's `unsafe`
boundary and would conflict on every future sync for zero benefit to us. So `clippy`
is a *read-it-yourself* lint here, not a pass/fail gate — but check that any new
error names a file **we** own before dismissing it. If the count moves off 8, look.

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
git rev-list --left-right --count upstream/main...xieyt/5
#  -> "<N behind>  <M ahead>"   ; M = number of commits we carry on top

# what exactly do we carry on top of upstream?
git log --oneline upstream/main..xieyt/5
git diff --stat upstream/main..xieyt/5

# structural tripwires — any hit means sync now, not later (UPSTREAM-SYNC.md §7)
git diff --stat HEAD..upstream/main -- Cargo.toml Cargo.lock crates/
git log --oneline HEAD..upstream/main -- src/layout_engine/systems/scrolling.rs
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
   because nothing *ran* the tests.
   - **`default_config_parses()` — DONE.** It lives in `src/common/config.rs`, parses
     the embedded `rift.default.toml`, asserts the keymap is non-empty, and asserts
     the four bare `move_focus` binds still resolve through `MoveFocusArgs`'s
     `Repr::Bare` arm. Verified passing as
     `common::config::tests::default_config_parses`; it fails *by construction* if
     `Repr::Bare` is dropped, since the bare bind then stops deserializing and
     `Config::parse(..).expect(..)` panics. It sits inside
     `just test`'s `common::config` filter, so it actually runs. Note the doc claim
     preceded the test: this section asserted it existed for a while when it did not
     — see `UPSTREAM-SYNC.md` §4.6 on trusting our own documentation.
   - **Still open: the unwinder.** `failed to initiate panic` means a panic aborts
     the whole run instead of reporting `FAILED`, so one bad test hides the rest. We
     hit this **three times** during the 2026-08 merge. `Cargo.toml` already sets
     `panic = "unwind"` and upstream's test overhaul did not fix it.
   - **Still open: the window-server seam.** Seam it behind the existing
     `testing.rs` mock so reactor/layout tests run headless; upstream added nothing
     here.
   - **Still open: `nix flake check`.** `just test` is not wired into it.
   - **`ui::hints_bar` allowlist hole — DONE** (2026-08-08). The filter was added to
     `just test`, taking it from 353 to 359 tests; §2.E's six geometry/click tests
     had silently never run. The general lesson stands and is why this item is worth
     keeping in sight: a curated allowlist fails *silent*, so adding a test module
     without adding its filter is a no-op that looks like coverage.
2. **Panic hygiene in runtime paths.** A WM should degrade, not crash. Partially
   advanced: §2.F's `CGPreflightScreenCaptureAccess` gate means a denied Screen
   Recording grant now degrades with one clear warning instead of a silent blank
   grid. The `.unwrap()`/`.expect()` audit continues on the remaining fallible paths
   (`Config::default`, AX calls). Note upstream still writes
   `dirs::home_dir().unwrap()` in `config.rs` (three sites) — our non-panicking
   `home_dir()` override must be defended at every merge (§4 item 6).
   `reload_config` must still validate-and-keep-old on a bad edit.
3. **AX/SkyLight race hardening.** The recurring hang/crash class (raise
   timeouts, focus-follows-mouse, sleep/display-churn, floating ping-pong).
   `raise_manager` already has good deterministic tests — extend that pattern to
   the churn/timeout/cancel paths.
4. **Property-test the layout math.** bsp constraints, scrolling offsets, master
   ratio are pure functions — proptest invariants (no overlaps, widths sum to the
   tiling area, focus always resolvable) catch geometry regressions cheaply.
