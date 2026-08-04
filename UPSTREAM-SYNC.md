# Upstream sync roadmap — 2026-08

Companion to `FORK.md`. `FORK.md` says *what we carry and why*; this says *how we
absorb the 38 upstream commits we are behind*, commit by commit, with a verdict
for each: do we need it, do we already have it, or is ours better.

Baseline measured 2026-08-05 on branch `xieyt/5`.

---

## 1. Situation (measured, not estimated)

```
merge-base          dd1e2bea1119d04b2d77ef98766c8dd2808d12e9
we carry            31 commits ahead
we are behind       38 commits (linear, zero merge commits upstream)
upstream churn      78 files, +7559 / -5343
```

Full-jump conflict surface (`git merge upstream/main`, rerere off): **8 files,
16 hunks**. That is small — the fork's quarantine discipline (`FORK.md` §5)
worked. The cost is not conflict resolution, it is three structural changes:

| Structural change | Impact on us |
|---|---|
| Repo becomes a **cargo workspace** with `crates/rift-protocol` + `crates/rift-client` | `LayoutCommand`, `ReactorCommand`, `ConfigCommand`, `DisplaySelector`, `Direction`, `LayoutMode`, `WorkspaceSelector`, `AnimationEasing` all **leave `src/`**. Five of our command variants must be re-homed across a crate boundary. Nix build scope must be narrowed. |
| `EventResponse` gains `changed: bool` | ~22 literals in `engine.rs` that we already annotated with `activate` need both fields. Compiler-enumerated (`E0063`). |
| Mission Control moves to **ScreenCaptureKit** | New macOS 12.3+ SDK requirement and a **new TCC permission class** (Screen Recording) our `.app`/nix packaging does not yet satisfy. |

`git rerere` does **not** help this sync. The 3 cached resolutions in
`.git/rr-cache` are from the previous sync and match none of these hunks —
`FORK.md` §3's claim that "the §4 resolutions are already recorded" is stale.

### The decisive measurement

Merging upstream in **chronological checkpoints** instead of one jump:

| Checkpoint | Commits | Conflicts (cumulative from `xieyt/5`) | Incremental |
|---|---|---|---|
| `ccfcc45` | 13 | **0 files, 0 hunks** | 0 |
| `b6deef3` | 22 | 4 files, 6 hunks | +6 |
| `6e5ed78` | 27 | 5 files, 8 hunks | +2 |
| `e546861` | 33 | 7 files, 14 hunks | +6 |
| `upstream/main` | 38 | 8 files, 16 hunks | +2 |

**Stage 1 was executed in a throwaway worktree and verified end to end:**
13 upstream commits merged with **zero conflicts, zero `cargo check` errors, and
327/327 curated tests passing** — including our own
`tab_segments_tile_the_bar_without_gaps_or_overlap` and
`scrolling_columns_are_contiguous_and_non_overlapping`. Upstream content
(`focus_desktop_window`, `MoveWindowToWorkspace.follow`, `EventOutcome::no_change`,
`menu_bar_inset`) and our content (`MoveFocusArgs`, hints-bar feed) both survived.

That is a third of the backlog for free.

---

## 2. Strategy

**Staged merge, five checkpoints.** Keep `FORK.md` §3's merge-not-rebase policy —
this refines *granularity*, not direction.

Why staged over one jump:
- Stage 1 is provably free. Landing it immediately shrinks the risky remainder
  from 38 commits to 25 and makes every later `git bisect` meaningful.
- The clusters **interleave chronologically**, so per-cluster cherry-picking is
  wrong: `12e8efc` needs `EventResponse.changed` from `b9b79be`, and `155d522`
  needs an `event_space` binding introduced by `a5fa216`. Chronological
  checkpoints respect every dependency by construction.
- Each stage compiles and tests on its own, so a regression is attributable to
  ≤9 upstream commits instead of 38.
- The two genuinely hard stages (2 and 4) get isolated commits, which is what
  `rerere` needs to be useful at the *next* sync.

Cost of staging: 5 merge commits instead of 1. Irrelevant — we never rebase this
branch.

```bash
git fetch upstream
git push origin upstream/main:main          # keep the clean mirror current
git checkout xieyt/5
for s in ccfcc45 b6deef3 6e5ed78 e546861 upstream/main; do
  git merge $s                              # resolve per §4 below
  cargo check --lib && just test            # gate: both must pass
  git commit                                # one merge commit per stage
done
git push origin xieyt/5
```

---

## 3. Verdicts — all 38 upstream commits

`TAKE` = adopt as-is · `REWORK` = adopt, re-apply our layer on top ·
`DROP-OURS` = adopt and delete redundant local code.

### Stage 1 — `ccfcc45` · 13 commits · 0 conflicts · **verified green**

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `a5fa216` | track role window (#242) | TAKE | Focuses the Finder desktop window when a workspace empties (`SLSManagedDisplaysCopyRoleWindows`). Test-stubbed to `false`, so headless-safe. Prerequisite for `155d522` and `6c64d8b`. |
| `af9fff7` | follow window to workspace | TAKE | `MoveWindowToWorkspace.follow`. Move-and-switch. Interacts with §2.B: with `activate_on_focus`, follow now foregrounds the target app — desirable, smoke-test it. |
| `ef39a1b` | StackLineHoverMode reversed | TAKE | Two-line polarity fix in code we own files for (§2.C) but never touched. We had this bug. |
| `93a1fd3` | no menubar gap on non-notched displays | TAKE | Real bug we also had; our `sys/screen.rs` was byte-identical to the pre-image. |
| `6a2b3ad` | less redundant layout arrangements | TAKE | Frame acks / no-op frames stop requesting an arrange. Free win for the hint bar. |
| `c6ebf61` | correct EventOutcome semantics | TAKE | `no_change`/`layout_changed`/`window_membership_changed`/`focus_changed` replace `finalized_event`. Touches `EventOutcome`, **not** our `activate` (which lives on `EventResponse`). |
| `c160d6c` | ghost windows from layout restore | TAKE | Persistence-only; we ceded that area upstream already. |
| `eb05b70` | restore space logic | TAKE | Ditto; strictly after `c160d6c`. |
| `38572a0` | event_taps going offline (#431) | TAKE | Tap self-healing via CFMachPort invalidation. Real reliability fix we lack. |
| `fa4ca62` | consume gesture events for a swipe | TAKE | We don't touch `gesture_tap.rs`. |
| `7f26efe` | slow workspace switch after activation | REWORK | Auto-switch returns an `EventOutcome`. See §4.1 — suppress `activate` on the auto path. |
| `8e7f9c6` | cleanup tests | TAKE | **Purely additive to `testing.rs`** — zero helpers deleted or renamed. |
| `ccfcc45` | cleanup tests^2 | TAKE | Ditto. Requires `8e7f9c6`. |

### Stage 2 — `b6deef3` · 9 commits · +6 hunks · **the hard one**

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `95bc739` | configurable window insertion point (#427) | TAKE | New `BaseLayoutSettings` flattened into every layout table. Conflicts with our `[settings.layout]` keys by adjacency only — no name collision. |
| `e95944e` | layout normalizations (#430) | TAKE | Sway-style `equalize_nodes`. `traditional.rs` only; no scrolling interaction. Needs `95bc739`. |
| `652a53f` | app activation handling | REWORK | **Highest-risk commit in the sync.** See §4.1. |
| `fe97ae2` | scope workspace switch to active display | REWORK | Adds `space_scope: Option<SpaceId>` to `update_layout`/`calculate_layout`/`update_layout_or_warn`. Must thread through our hints-bar reserve-convergence recursion (`managers.rs`) or a scoped switch silently degrades to a full re-layout. |
| `a369cf2` | default equalize_nodes to true | TAKE | Default flip. |
| `b9b79be` | recognize layout noops | REWORK | Adds `EventResponse.changed`. Union with our `activate` across ~22 literals. Compiler-enumerated. Pleasant interaction: our `switch_to_workspace` already returns `default()` for a no-op switch, so `changed=false` **and** `activate=false` — the new early-return also suppresses a redundant `NSRunningApplication::activate()`. |
| `fe8a687` | remove duplicate menu update | TAKE + **DROP-OURS** | Delete our duplicate `maybe_send_menu_update()` in `managers.rs`. |
| `a5c429d` | position only workspace switch | TAKE | New `SetWorkspaceSwitchPositions` AX batch. Same `managers.rs` hunk as `fe8a687`; take after it. |
| `b6deef3` | fix tests | TAKE | Needs `e95944e`. |

### Stage 3 — `6e5ed78` · 5 commits · +2 hunks · **packaging gate**

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `5c0b38d` | focused window changed broadcast | TAKE | Take for *external* consumers. **Do not** re-point the hint bar at it — see §5.2. |
| `3185d1c` | make animations respect config (#432) | TAKE | `animation_fps`/`animation_duration` were hardcoded. Real bug we had. |
| `a90e043` | move sponsor button | TAKE | Cosmetic. |
| `0bf5549` | improvements to mission control 1/n | REWORK | ScreenCaptureKit rewrite. **Blocked on the packaging work in §4.2.** |
| `6e5ed78` | insertion point in scrolling layout | TAKE | Routes app-reconciliation through the insertion policy. Orthogonal to §2.D: never reads `Column.tabbed`/`active`/`width_offset`/`tab_groups`. |

### Stage 4 — `e546861` · 6 commits · +6 hunks · **protocol re-homing**

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `25d5923` | rift-client library | TAKE | Cargo workspace + Mach transport extraction. Apply the nix change in the **same** commit (§4.3) or the build breaks. |
| `8f0d925` | dimmer example | TAKE | Free; excising costs more than carrying. Its `ctrlc`→`nix 0.31.3` dev-dep subtree is kept out of the nix build by the `--package` scoping in §4.3. |
| `7fc1089` | match higher max message size | TAKE | Mandatory: `MAX_MESSAGE_SIZE` 16 KiB → 256 KiB, or the server truncates typed payloads. |
| `a39581e` | rift-protocol | REWORK | Wire format changes shape: `ExecuteCommand{command: RiftCommand}`, `Subscribe{event: EventKind}`, `GetWindowInfo{window_id: WindowId}`. Our four `RiftCommand::Reactor` sites in `rift-cli.rs` become `CliCommand::Reactor`. |
| `103af15` | handling of legacy untyped clients | TAKE | Back-compat shim for the *old* wire format — i.e. ours. Skipping it breaks every stale `rift-cli` and any sketchybar/Lua consumer. |
| `e546861` | typed event data | REWORK | The big re-homing. See §4.4. |

### Stage 5 — `upstream/main` · 5 commits · +2 hunks

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `12e8efc` | roll strip automatically on focus change (#434) | TAKE | Needs `b9b79be`'s `changed`. **Not** a re-add of the off-left snap we reverted — it touches zero lines of `scrolling.rs`; upstream's offset clamp is unchanged. Keep our revert. Complementary to §2.D: it is what makes our per-column tab memory visible on non-keyboard focus paths (Dock, IPC). |
| `155d522` | mouse_hides_on_focus (#374) | TAKE | Needs `a5fa216`. |
| `b619ce5` | hot reload non-keybinding settings | REWORK | `service_config_update: Option<(Config,bool)>` → `Option<Config>`. **Certain conflict:** upstream rewrites the exact block holding our `hints_bar_tx → ConfigUpdated` arm. A careless "take theirs" silently deletes hints-bar hot reload. Bonus: `event_tap`'s cached `stack_line_*` settings finally hot-reload without a keybinding change. |
| `151d99f` | rift is stable | TAKE | README wording. |
| `6c64d8b` | more app rule fields (#375) | REWORK | See §5.1 — partially delivers our roadmap item 3. Adds `Column.width_overridden`, which needs a mechanical sweep of our 21 exhaustive `Column` literals in `scrolling.rs`. |

**Net: 38 TAKE, 0 SKIP.** Nothing upstream added is unnecessary, and nothing
upstream added supersedes a local feature. Exactly one line of ours gets deleted
as redundant (the duplicate `maybe_send_menu_update`).

---

## 4. The five pieces of real work

Everything else is mechanical. These are the ones that need judgment.

### 4.1 §2.B `activate` vs upstream's activation rework — *the one semantic conflict*

Upstream does **not** foreground applications and is not moving that way.
Verified: `NSRunningApplication::activate` / `activateWithOptions` appear nowhere
in upstream's `src/actor/app.rs` or `src/sys/app.rs`; upstream's `RaiseRequest`
is still the 4-tuple `(wids, token, sequence_id, quiet)` against our 5-tuple. The
one `app.activate()` upstream added is `NSApplication::sharedApplication(mtm)` in
the Mission Control overlay — Rift activating *itself* to take key focus, not
per-app foregrounding. **§2.B stays, in full, and is not even partially
duplicated.**

But `652a53f` + `7f26efe` change the ground under it. `652a53f` makes the Carbon
front-process edge authoritative and routes it through `on_global_activation`;
`7f26efe` makes `ApplicationActivated(pid, Quiet::No)` absorb an auto workspace
switch. Our `running_app.activate()` fires *before* `wait_for_activation` arms
`last_activated`, so the Carbon edge our own call produces can be classified
"by user" → `Quiet::No` → spurious auto workspace switch. Two fixes:

1. Arm the quiet marker (`pending_activation_quiet` / `last_activated`) **before**
   calling `running_app.activate()`, so upstream attributes the edge to us.
2. Suppress `EventResponse.activate` on the auto-switch path — that switch is a
   *reaction* to an activation, so re-foregrounding the app that just came
   forward is at best a wasted `ActivateIgnoringOtherApps`.

Not an infinite loop today (`652a53f` adds `is_globally_frontmost` dedup and
drops the "any standard window of this pid" fallback), but it is a real
behavioural regression risk and the merge's only genuine design decision.
Both fixes are runtime-only — `cargo check` and `just test` will not catch them;
verify with `just install` and a Dock-click on an app living on another workspace.

### 4.2 ScreenCaptureKit packaging (blocks `0bf5549`)

Mission Control's rewrite turns a pure-code change into a packaging + permissions
change for our `.app` bundle:

- **SDK (build-breaking):** `nix/package.nix` passes `buildInputs = [ ]` and
  inherits `clangStdenv`'s default `apple-sdk` (11.x). ScreenCaptureKit needs
  **12.3+**. Add `pkgs.apple-sdk_15` to `buildInputs` *and* to the devShell
  packages, with a matching `darwinMinVersion`/`-mmacosx-version-min=12.3`.
- **Frameworks:** no explicit `-framework` flags — the `objc2-*` crates emit
  `#[link(kind="framework")]`. SDK alone is sufficient. `strictDeps = true` stays
  valid (these are `buildInputs`).
- **Info.plist — two files:** `assets/Info.plist` *and* the heredoc plist
  generated in `nix/package.nix` (the one actually shipped). Both need
  `NSScreenCaptureUsageDescription`. Without the purpose string macOS **kills the
  process** at the TCC prompt instead of showing it.
- **No entitlement exists** for screen capture — it is purely TCC-gated
  (`kTCCServiceScreenCapture`). No `.entitlements` file, and App Sandbox must
  stay off.
- **TCC persistence:** our stable `rift-codesign` identity story carries over
  (same identity+bundle-id keying as Accessibility), but Screen Recording is a
  **separate consent record** — every user gets a new prompt on first launch after
  this merge, from a launchd agent at login, which is easy to miss. Document it
  in `nix/module.nix` and the README. Ad-hoc (`signingIdentity = "-"`) installs
  will be re-prompted on every rebuild, same as Accessibility today.
- **Recommended hardening (ours, not upstream's):** gate capture on
  `CGPreflightScreenCaptureAccess()` so a denied grant degrades to blank previews
  instead of hanging. This is exactly `FORK.md` §8 stability item 2 (a WM should
  degrade, not crash).

### 4.3 Cargo workspace → nix build scope (must land with `25d5923`)

- `src` filtering needs **no change**: `craneLib.fileset.commonCargoSources ../.`
  is recursive over `*.rs`/`*.toml`/`Cargo.lock`, so `crates/**` is captured.
- `pname`/`version` need **no change**: the root keeps `[package] rift-wm`, so
  `crateNameFromCargoToml` still resolves (a virtual manifest would not have).
- `flake.nix` needs **no change** — it holds no cargo/crane knobs.
- **Required:** add `cargoExtraArgs = "--locked --package rift-wm --bins";` to the
  shared `args` attrset, so `buildDepsOnly` and `buildPackage` agree. It must be
  in `args`, not just the `buildPackage` call — crane keys `cargoArtifacts` reuse
  on the argument set, and a mismatch silently rebuilds every dependency. This
  also keeps the `rift-client` examples' `ctrlc`/`nix 0.31.3`/`dispatch2` subtree
  out of the build.
- `Cargo.lock`: take upstream's wholesale (our fork adds no dependencies). crane
  defaults to `--locked`, so a stale lock is a hard failure, not a silent
  re-resolve.
- `resolver = "3"` needs cargo ≥1.84; we are already on `edition = "2024"`
  (≥1.85), so the pinned fenix toolchain suffices.
- Recommended: add `-p rift-wm` to `justfile`'s `test` recipe so `just test` does
  not compile `rift-client`'s SkyLight-linking `build.rs` for zero tests.

### 4.4 Re-homing our command variants into `rift-protocol` (`e546861`)

| Ours | From | To |
|---|---|---|
| `LayoutCommand::MoveFocus(MoveFocusArgs)` **+ the struct + its custom `Deserialize`** | `layout_engine/engine.rs` | `crates/rift-protocol/src/commands.rs` |
| `LayoutCommand::FocusColumn(usize)` | `layout_engine/engine.rs` | same |
| `LayoutCommand::ToggleColumnTabbed` | `layout_engine/engine.rs` | same |
| `LayoutCommand::CycleColumnWidth` | `layout_engine/engine.rs` | same |
| `ReactorCommand::ToggleHintsBar` | `model/reactor.rs` | same |

Match arms, clap definitions, and the `WmCmd::ToggleHintsBar` bridge all stay in
`src/`. `rift-protocol` depends only on `serde`/`serde_json`, and
`MoveFocusArgs { direction, activate }` ports cleanly.

**The `MoveFocusArgs` landmine.** It *must* port, and it must keep its dual-form
`Deserialize` (`Repr::Bare(Direction)` | `Repr::Full { direction, activate }`),
because `rift.default.toml` binds the bare `move_focus = "left"` and that file is
parsed by `Config::default().unwrap()`. Upstream's `103af15` puts
`#[serde(rename = "direction")]` on its `MoveFocus(Direction)` newtype — that
attribute occupies exactly the slot our struct replaces. Keeping upstream's
`MoveFocus(Direction)` while `engine.rs` still destructures
`MoveFocusArgs { direction, activate }` gives `E0308`; porting the struct but
dropping `Repr::Bare` gives a **runtime panic in `Config::default()` that
`cargo check` will not catch**. This is the `FORK.md` §2.B trap, one crate to the
left. Our version is a strict superset of upstream's and stays.

Side effect worth noting: once `MoveFocusArgs` lives in `rift-protocol`, our
`activate` flag becomes part of the *public* IPC vocabulary, and `103af15`'s
legacy decoder routes old `{"Reactor":{"move_focus":"left"}}` payloads through
the same deserializer — one impl, two back-compat duties. That is a good outcome,
but it means `activate` is now an API we own compatibility for.

**Corrected finding:** a scout flagged `MoveWindowToWorkspace.follow` losing its
`#[serde(default)]` in the move as a second `Config::default()` panic. Verified
false — the bare `move_window_to_workspace = 0` binding resolves through
`WmCmd::MoveWindowToWorkspace(WorkspaceSelector)` under
`#[serde(untagged)] WmCommand`, never reaching `LayoutCommand`. Upstream's own
default TOML uses the bare form and does not panic. Adding `#[serde(default)]` is
still correct hygiene for the documented *table* form
(`{ workspace = N, window_id = 123 }`, which post-merge would demand `follow`)
and for legacy IPC clients — do it, but it is not a panic risk.

### 4.5 Mechanical sweeps (compiler-enumerated)

| Upstream change | Our callsites |
|---|---|
| `EventResponse` gains `changed: bool` | ~22 literals in `layout_engine/engine.rs` (union with `activate`); destructure in `reactor.rs` needs `changed: _`; two `EventResponse` literals in `reactor/tests.rs` need `changed: true` |
| `Column` gains `width_overridden: bool` | **21** exhaustive `Column { .. }` literals in `systems/scrolling.rs` (ours also carry `tabbed` + `active`) |
| `space_scope: Option<SpaceId>` on `update_layout`/`calculate_layout`/`update_layout_or_warn(_with)` | `managers.rs` (incl. the reserve-convergence recursion), 3 `reactor.rs` sites, 1 test |
| `service_config_update: Option<(Config,bool)>` → `Option<Config>` | `events/outcome.rs` (4 sites), `events/command.rs`, `reactor.rs` destructure |
| `ReactorCommand::FocusWindow.window_id` → `rift_protocol::WindowId` | 2 sites in `actor/hints_bar.rs` (chip-click + detail overlay) — add `.into()` |
| `MissionControlActor::new` gains a `Sender` | 1 site in `bin/rift.rs` |
| `Animation::new()` → `Animation::new(Config)` | upstream-internal only |
| `CgsWindow::bind_to_context` → `bind_layer_context` (unsafe) | **grep `ui/stack_line.rs` + `ui/hints_bar.rs` before merging** — both own CGS windows |
| `window_server::{CapturedWindowImage, capture_window_image, resize_cgimage_fit}` **removed** | grep before merging |
| `MainWindowTracker::take_global_activation_quiet` → `is_globally_frontmost` | 1 site in `reactor.rs` |
| `EventOutcome::{finalized_event, activate_application, with_application_activation}` **removed** | delete our copies; no local callers |
| `layout_engine.rs` re-export | 3-way union: our `MoveFocusArgs` + upstream's `LayoutEventOutcome` + `Restore*` |

`just test`'s allowlist survives intact — all five module-path filters
(`layout_engine`, `actor::raise_manager`, `actor::reactor::tests`,
`common::config`, `ui::stack_line`) still resolve, and the known-broken
`topology_change_clears_stale_pending_hide_target_*` still exists upstream, so
keep the `--skip`.

---

## 5. Where we stand relative to upstream after the sync

### 5.1 Our features vs upstream

| Ours | Status after sync |
|---|---|
| §2.A nix packaging | Fork-only. Upstream still ships no Nix support. Gains real work (§4.2, §4.3). |
| §2.B app activation on focus | **Fork-only and unique.** Upstream deliberately never foregrounds apps. Needs §4.1 rework. |
| §2.C stack-line tabbed titles | Fork-only. No upstream equivalent. |
| §2.D niri tabbed columns (scrolling) | **Fork-only, and strictly ahead.** Upstream shipped no tabbed-column or per-column-width-preset equivalent. `12e8efc` and `6e5ed78` are complementary — they make our tab memory *more* visible, not redundant. |
| §2.E hint bar | Fork-only. Hot reload needs defending through `b619ce5`. |
| Per-column width presets (`76195ff`) | Fork-only. Roadmap item 2, still ours. |
| Panic hygiene (`c32893b`) | **Ours is better and must be defended.** Upstream still has `dirs::home_dir().unwrap()` in `config.rs`; our non-panicking `home_dir()` helper appears on the *our* side of a conflict hunk. Do not "take theirs" there. |
| Property tests (`46a983a`), raise_manager hardening (`4d97013`) | Fork-only. Verified still green post-stage-1. |

**One genuine design disagreement.** Upstream `95bc739`-era `scrolling.rs` adds:
horizontal `move_node` on a multi-window column **extracts** the focused window
into its own neighbour column ("a faster way to undo accidental stacks"). Our
`dcc768d` + `5e5916f` deliberately do the opposite — a column moves *as a whole*,
and you use consume/expel to move windows in or out. This is the one place where
"take upstream's structure" is the wrong instinct.

Recommendation: **split the behaviour on `Column.tabbed`.** A tabbed column is a
single unit by definition, so keep ours there (`dcc768d`). For a non-tabbed
vertical stack, take upstream's extract behaviour and **drop `5e5916f`** — that
commit extended "whole column" to plain stacks, which is a preference, and
upstream's is defensible and cheaper to carry. Update the two affected tests
(`horizontal_move_shifts_non_tabbed_stack_as_a_whole` becomes an
extract-assertion).

### 5.2 Upstream features we should *not* adopt into our subsystems

- **`5c0b38d` focused-window broadcast is not a cheaper hint-bar feed.** Take the
  commit (external consumers want it — upstream's own `dimmer` example uses it),
  but keep §2.E's per-layout-apply feed. The broadcast payload is geometry-free
  (`window_id`, `workspace_*`, `space_id`, `display_uuid`), while the bar needs
  per-column composition *and* pixel frames. Worse, it fires on a strict subset:
  a scroll, a column resize, a window opening in a non-focused column, and an
  overflow re-split all change bar content **without** changing the focused
  window. A broadcast-driven bar renders stale.
- **Our hint-bar perf work stays, all four layers.** Upstream's `6a2b3ad` /
  `b9b79be` cut the *number of arranges*; our dedup cuts *sends per arrange*.
  Different pipeline stages. Ours still uniquely covers multi-pass arranges
  (`passes` up to 3), genuine layout changes with identical bar content (resize,
  gaps, config reload), our own reserve-convergence second pass, and burst
  coalescing during a scroll gesture. Keep the `hbbench` counters — they are the
  instrument for re-running the `FORK.md` §6 A/B and quantifying how much of the
  win migrated upstream.
- **`93a1fd3` gives us no notch API to migrate to.** Its `menu_bar_inset` is
  private and scalar; the pre-existing `System::notch_height` is also scalar. Our
  `HintsBar::compute_notch_gap` needs *horizontal* extents
  (`auxiliaryTopLeftArea`/`auxiliaryTopRightArea`), so `flow`/`stop` cannot be
  built on either. Keep ours; add a comment cross-referencing
  `sys::screen::System::notch_height` as the sibling implementation, and consider
  borrowing its `CGDisplayIsBuiltin` guard, which we lack.

### 5.3 `FORK.md` §8 roadmap — what upstream delivered

| Item | Status |
|---|---|
| 1. Tabbed column display | Shipped by us (§2.C/§2.D). Upstream: nothing. |
| 2. Per-column width presets | Shipped by us (`76195ff`). Upstream: nothing. |
| 3. Richer window-rule open actions | **Partly upstream** (`6c64d8b`). `open_floating` existed as `floating: bool` and now gets real adoption-time effect via `AppRulePlacement::resolve_frame`; `default_column_width` arrives by proxy as tiled `size.w` **in pixels, not a ratio**; `open_maximized` **not** delivered. Bonus new field: `focus: bool`. Also, `95bc739`'s `BaseLayoutSettings` + `resolved_base_for(mode)` is the exact override-resolution seam a per-rule open-action would extend — item 3 got cheaper. |
| 4. Workspace reordering / on-the-fly named workspaces | Not delivered. `af9fff7`'s `follow` is adjacent ergonomics, not this. |
| 5. Interactive mouse resize of tiles | Not delivered. |

**Stability roadmap item 1 is still fully open**, and one claim in it is wrong:
`FORK.md` §8 says "keep `default_config_parses()` green" — **that test does not
exist anywhere in the repo.** `Config::default()` is covered only incidentally,
via `reactor/testing.rs`. Since a panic aborts the whole test binary, a bad
`rift.default.toml` currently kills the run with no attributable failure. Given
that `95bc739` puts `#[serde(flatten)]` on `deny_unknown_fields` structs and §4.4
moves `MoveFocusArgs` across a crate boundary — both silent-until-runtime — this
test is now the cheapest high-value thing on the list. Write it.

Upstream's test overhaul does **not** help here: `Cargo.toml` is untouched by it
(`panic = "unwind"` was already set), the `failed to initiate panic` abort
remains, and no window-server seam was added.

---

## 6. Ordered plan

**Phase 1 — land the free third.** Merge `ccfcc45`. Already verified: 0 conflicts,
0 `cargo check` errors, 327/327 tests. Push. *(Nothing to decide.)*

**Phase 2 — the layout/perf core.** Merge `b6deef3`. Union `EventResponse`
(`changed` + `activate`), thread `space_scope` through the hints-bar
reserve-convergence recursion, delete the duplicate `maybe_send_menu_update`,
resolve `[settings.layout]` and `ScrollingLayoutSettings` by union. Then §4.1:
re-order our `running_app.activate()` behind the quiet marker and suppress
`activate` on the auto-switch path. Gate: `cargo check` + `just test` + a
`just install` smoke test of Dock-activating an app on another workspace.

**Phase 3 — packaging prerequisite, then UI.** Do §4.2 **first** (apple-sdk_15,
both Info.plists, `CGPreflightScreenCaptureAccess` gate, module/README note),
then merge `6e5ed78`. Gate: `nix build .#rift` + `just install` + Mission Control
shows real previews, not black. Expect a new Screen Recording prompt.

**Phase 4 — protocol re-homing.** Apply §4.3's `cargoExtraArgs` in the same
commit as `25d5923`, then merge through `e546861` doing §4.4. Gate: `just smoke`
(proves both binaries link and run), plus an **old** `rift-cli` against the new
`rift` to prove `103af15`'s legacy path works, plus a `move_focus = "left"`
keybind actually firing (the `Repr::Bare` path).

**Phase 5 — tail.** Merge `upstream/main`. Defend the `hints_bar_tx` arm through
`b619ce5`'s rewrite; sweep the 21 `Column` literals for `width_overridden`; take
`12e8efc`. Gate: full `just test` + `just install`; re-run the `FORK.md` §6
`hbbench` A/B and record how much of the hint-bar win migrated upstream.

**Phase 6 — cleanup.** Write `default_config_parses()`. Settle the §5.1
horizontal-move split and update its two tests. Update `FORK.md`: §3's stale
rerere claim, §8's phantom test, §8 item 3's new partial-upstream status, and add
`Column.width_overridden` / `EventResponse.changed` / the protocol-crate re-homing
to the §4 conflict playbook so the *next* sync is cheaper. Commit the rerere
cache.

---

## 7. Keeping current after this

The 38-commit backlog cost this much analysis because we let it accumulate. The
measurement that matters: **stage 1's 13 commits merged for free.** Upstream
commits are individually small and mostly non-overlapping with our quarantined
features; it is the *structural* commits (workspace conversion, `EventResponse`
field, framework swap) that cost, and those cost the same whether you take them
early or late — except that taking them late means resolving them against a
larger local delta.

So: **sync at every upstream structural change, not on a calendar.** Concretely,
run `git fetch upstream && git rev-list --count HEAD..upstream/main` weekly and
merge whenever the count exceeds ~10 or a commit touches `Cargo.toml`,
`EventResponse`, `EventOutcome`, or adds a crate. Commit the rerere cache
(`.git/rr-cache` is local-only today — that is why it was useless this time;
consider vendoring the resolutions into this file's §4 instead).
