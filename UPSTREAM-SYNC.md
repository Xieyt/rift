# Upstream sync — 2026-08 · completed record

Companion to `FORK.md`. `FORK.md` says *what we carry and why*; this says *how we
absorbed the 38 upstream commits we were behind*, commit by commit, with the
verdict each one got and what it actually cost.

Written as a plan on 2026-08-05 and executed the same day. It is kept as the
reference for the **next** sync, not as history for its own sake:

- **§3** — why each of the 38 commits was taken. Unchanged verdicts; notes now
  say where the predicted rework differed from the real one.
- **§4** — the five pieces of real work, as landed. **§4.6 is the predictions that
  were wrong; §4.7 is what live testing found afterwards.** Read both before
  trusting anything else here.
- **§6** — per-stage outcome log (conflicts, resolutions, gates).
- **§7** — the sync cadence the measurements actually support.

## 0. Status — merge complete, and one follow-on since

```
branch                    xieyt/5
behind upstream           0        (git rev-list --left-right --count HEAD...upstream/main)
ahead                     44
upstream tip merged       1de4d09  (the 38-commit backlog ended at 6c64d8b)
curated tests             327 -> 352 (merge) -> 353 (guard test) -> 359 (allowlist fix)
cargo check               --workspace --all-targets clean
just fmt-check            clean
nix build .#rift          succeeds; .app installed and exercised live
```

Landed as five chronological checkpoint merges, one per stage. The checkpoint
SHAs are *upstream* commits; the merge commits on `xieyt/5` are separate:

| Stage | Upstream checkpoint | Merge commit on `xieyt/5` | Upstream commits | Conflict hunks resolved | Curated tests after |
|---|---|---|---|---|---|
| 1 | `ccfcc45` | `84382a7` | 13 | **0** | 327 |
| 2 | `b6deef3` | `69dd857` | 9 | 6 | 343 |
| 3 | `6e5ed78` | `e270ce2` | 5 | 2 | 345 |
| 4 | `e546861` | `4a2e3b5` | 6 | 6 | 345 |
| 5 | `upstream/main` (`6c64d8b`) | `4e8b56e` | 5 | 2 | 352 |

Each merge commit message carries its own resolution log — those messages, not
this file, are the authoritative account of what was changed and why. Fork commits
added alongside and after the merges: `84ea8e8` (§4.2 packaging), `d1622e6`
(Mission Control Screen-Recording preflight), `1c73bb3` (`default_config_parses`),
`54ce476` (`just fmt`/`clippy` routed through `nix develop`), `9f26883` (nightly
rustfmt pass over 13 fork-owned files).

**Follow-on merge, 2026-08-08 — `1de4d09` → `45e7826`.** Upstream #437, "move_node
hiding col in scroll layout": after `move_selection`, niri navigation reveals the
moved column (`reveal_selected_without_direction`) instead of re-aligning
(`align_scroll_to_selected`). Ten lines, zero conflicts, and directly relevant —
`focus_navigation_style = "niri"` is the configuration §2.D/§2.E target. It lands in
`move_selection`, one level above our §5.1 `move_selected_window_horizontal` split,
so the two are independent. See `FORK.md` §2.D.

**Post-merge live verification (2026-08-08).** `just install` on the real desktop,
then exercised end to end: typed IPC round-trip; `focus-column N`; `toggle-tabbed`;
`cycle-column-width` (widths matched the configured presets exactly); **both branches
of the §5.1 split** (tabbed column moved intact, untabbed stack extracted);
`--activate` foregrounding plus the §4.1 spurious-switch hazard (12 activations with
the app parked on two workspaces, zero jumps); hint bar rendering including the
overflow strip; and #437's reveal. Two things were *found* by this pass rather than
confirmed — see §4.7.

---

## 1. Situation at the start (measured, not estimated)

```
merge-base          dd1e2bea1119d04b2d77ef98766c8dd2808d12e9
we carry            31 commits ahead
we are behind       38 commits (linear, zero merge commits upstream)
upstream churn      78 files, +7559 / -5343
```

Full-jump conflict surface (`git merge upstream/main`, rerere off) measured
**8 files, 16 hunks**. That was small — the fork's quarantine discipline
(`FORK.md` §5) worked, and it held: the staged merges resolved 16 hunks total,
exactly the predicted count. The cost was never conflict resolution; it was three
structural changes:

| Structural change | Impact on us |
|---|---|
| Repo becomes a **cargo workspace** with `crates/rift-protocol` + `crates/rift-client` | `LayoutCommand`, `ReactorCommand`, `ConfigCommand`, `DisplaySelector`, `Direction`, `LayoutMode`, `WorkspaceSelector`, `AnimationEasing` all **leave `src/`**. Five of our command variants must be re-homed across a crate boundary. Nix build scope must be narrowed. |
| `EventResponse` gains `changed: bool` | ~22 literals in `engine.rs` that we already annotated with `activate` need both fields. Compiler-enumerated (`E0063`). |
| Mission Control moves to **ScreenCaptureKit** | New macOS 12.3+ SDK requirement and a **new TCC permission class** (Screen Recording) our `.app`/nix packaging did not satisfy. Landed as `84ea8e8`; see §4.2 — and §4.6, because the "build-breaking" half of this was wrong. |

`git rerere` did **not** help this sync. The 3 cached resolutions in
`.git/rr-cache` were from the previous sync and matched none of these hunks —
upstream had moved the surrounding code, so `FORK.md` §3's claim that "the §4
resolutions are already recorded" was stale and has been corrected. The cache now
holds 19 entries: those 3 stale ones, 4 from the exploratory full-jump probe, and
**12 fresh resolutions recorded across the five staged merges** (4 in stage 2, 1
in stage 3, 5 in stage 4, 2 in stage 5). Those 12 are the ones that pay off next
time — but only if the surrounding code has not moved again, which is the whole
argument for §7.

### The decisive measurement

Merging upstream in **chronological checkpoints** instead of one jump. Predicted
cumulative conflict surface, which the execution matched exactly:

| Checkpoint | Commits | Conflicts (cumulative from `xieyt/5`) | Incremental |
|---|---|---|---|
| `ccfcc45` | 13 | **0 files, 0 hunks** | 0 |
| `b6deef3` | 22 | 4 files, 6 hunks | +6 |
| `6e5ed78` | 27 | 5 files, 8 hunks | +2 |
| `e546861` | 33 | 7 files, 14 hunks | +6 |
| `upstream/main` | 38 | 8 files, 16 hunks | +2 |

**Stage 1 was pre-executed in a throwaway worktree and verified end to end**
before anything was committed: 13 upstream commits merged with **zero conflicts,
zero `cargo check` errors, and 327/327 curated tests passing** — including our own
`tab_segments_tile_the_bar_without_gaps_or_overlap` and
`scrolling_columns_are_contiguous_and_non_overlapping`. Upstream content
(`focus_desktop_window`, `MoveWindowToWorkspace.follow`, `EventOutcome::no_change`,
`menu_bar_inset`) and our content (`MoveFocusArgs`, hints-bar feed) both survived.
It then landed as `84382a7` with the same result.

A third of the backlog was free. That is the single most important number in this
document — see §7.

---

## 2. Strategy (as executed)

**Staged merge, five checkpoints.** `FORK.md` §3's merge-not-rebase policy held —
this refined *granularity*, not direction.

Why staged over one jump:
- Stage 1 was provably free. Landing it first shrank the risky remainder from 38
  commits to 25 and made every later `git bisect` meaningful.
- The clusters **interleave chronologically**, so per-cluster cherry-picking would
  have been wrong: `12e8efc` needs `EventResponse.changed` from `b9b79be`, and
  `155d522` needs an `event_space` binding introduced by `a5fa216`. Chronological
  checkpoints respect every dependency by construction.
- Each stage compiled and tested on its own, so a regression is attributable to
  ≤9 upstream commits instead of 38. This paid off immediately: the curated-test
  count is a per-stage number (327 → 343 → 345 → 345 → 352), so a new failure was
  always attributable to the stage that introduced it.
- The two genuinely hard stages (2 and 4) got isolated commits, which is what
  `rerere` needs to be useful at the *next* sync.

Cost of staging: 5 merge commits instead of 1. Irrelevant — we never rebase this
branch.

What was actually run, per stage:

```bash
git fetch upstream
git push origin upstream/main:main          # keep the clean mirror current
git checkout xieyt/5
for s in ccfcc45 b6deef3 6e5ed78 e546861 upstream/main; do
  git merge $s                              # resolve per §4
  cargo check --workspace --all-targets     # gate 1
  just test                                 # gate 2 — curated suite must pass
  git commit                                # one merge commit per stage
done
git push origin xieyt/5
```

**Correction to the original plan's gate:** it said `cargo check --lib`. That is
not enough. `--lib` skips the test targets, and the fork's exhaustive `Column` /
`EventResponse` literals live *in* test modules — the sweeps in stages 2 and 5 are
invisible to `--lib`. From stage 4 on, `--workspace` is also mandatory: `--lib`
alone no longer covers `crates/rift-protocol`, which is where our command variants
now live. Use `cargo check --workspace --all-targets`. Neither form catches the
`Config::default()` serde panic — only `just test` does (§4.4, §4.6).

---

## 3. Verdicts — all 38 upstream commits

`TAKE` = adopt as-is · `REWORK` = adopt, re-apply our layer on top ·
`DROP-OURS` = adopt and delete redundant local code.

All 38 landed. The verdicts below are unchanged from the pre-merge analysis — they
are the record of *why* each commit was taken. Notes marked **Reality:** are where
the predicted rework differed from what the merge actually needed.

### Stage 1 — `ccfcc45` → `84382a7` · 13 commits · 0 conflicts · **landed green**

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
| `7f26efe` | slow workspace switch after activation | REWORK | Auto-switch returns an `EventOutcome`. See §4.1 — suppress `activate` on the auto path. **Reality:** the rework landed in stage 2 (with `652a53f`, which it depends on), not stage 1: `reactor.rs` now sets `response.activate = false` on the `WorkspaceSwitchOrigin::Auto` path. |
| `8e7f9c6` | cleanup tests | TAKE | **Purely additive to `testing.rs`** — zero helpers deleted or renamed. |
| `ccfcc45` | cleanup tests^2 | TAKE | Ditto. Requires `8e7f9c6`. |

### Stage 2 — `b6deef3` → `69dd857` · 9 commits · 6 hunks · **the hard one, and it was**

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `95bc739` | configurable window insertion point (#427) | TAKE | New `BaseLayoutSettings` flattened into every layout table. Conflicts with our `[settings.layout]` keys by adjacency only — no name collision. |
| `e95944e` | layout normalizations (#430) | TAKE | Sway-style `equalize_nodes`. `traditional.rs` only; no scrolling interaction. Needs `95bc739`. |
| `652a53f` | app activation handling | REWORK | **Highest-risk commit in the sync, correctly called.** The only genuine design decision in the merge and the only resolution no test can verify. See §4.1 for what landed. |
| `fe97ae2` | scope workspace switch to active display | REWORK | Adds `space_scope: Option<SpaceId>` to `update_layout`/`calculate_layout`/`update_layout_or_warn`. Must thread through our hints-bar reserve-convergence recursion (`managers.rs`) or a scoped switch silently degrades to a full re-layout. **Reality:** exactly as predicted — `managers.rs::update_layout` passes `space_scope` unchanged into the convergence recursion. |
| `a369cf2` | default equalize_nodes to true | TAKE | Default flip. |
| `b9b79be` | recognize layout noops | REWORK | Adds `EventResponse.changed`. Union with our `activate` across ~22 literals. Compiler-enumerated. Pleasant interaction: our `switch_to_workspace` already returns `default()` for a no-op switch, so `changed=false` **and** `activate=false` — the new early-return also suppresses a redundant `NSRunningApplication::activate()`. **Reality: the ~22-literal sweep did not happen.** `changed` and `activate` sit at opposite ends of the struct, so git auto-merged nearly every literal. Exactly one needed a hand edit: our fork-only `FocusColumn` handler, which upstream has no counterpart for and so got no `changed` field. See §4.6. |
| `fe8a687` | remove duplicate menu update | TAKE (**not** DROP-OURS) | Predicted as "delete *our* duplicate `maybe_send_menu_update()` in `managers.rs`". **Reality: the `DROP-OURS` label was wrong — that line was upstream's.** Verified: `fe8a687^`'s `managers.rs` contains the call, `fe8a687`'s does not, and the commit relocates it into `reactor.rs`. It only *looked* like a fork addition because our local edits sit next to it — which is exactly how inherited code gets misattributed mid-merge. Nothing of ours was deleted here. Recorded in `FORK.md` §2's dropped list so the call is not re-added. |
| `a5c429d` | position only workspace switch | TAKE | New `SetWorkspaceSwitchPositions` AX batch. Same `managers.rs` hunk as `fe8a687`; take after it. |
| `b6deef3` | fix tests | TAKE | Needs `e95944e`. |

### Stage 3 — `6e5ed78` → `e270ce2` · 5 commits · 2 hunks · **packaging gate**

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `5c0b38d` | focused window changed broadcast | TAKE | Take for *external* consumers. **Do not** re-point the hint bar at it — see §5.2. |
| `3185d1c` | make animations respect config (#432) | TAKE | `animation_fps`/`animation_duration` were hardcoded. Real bug we had. |
| `a90e043` | move sponsor button | TAKE | Cosmetic. |
| `0bf5549` | improvements to mission control 1/n | REWORK | ScreenCaptureKit rewrite. **Blocked on the packaging work in §4.2.** **Reality:** the packaging (`84ea8e8`) was done first as planned and `nix build` succeeded — but it was never actually a *blocker*, only insurance. See §4.6, correction 1. The real fork-side cost of this stage was elsewhere: git interleaved our `scrolling.rs` test tail with upstream's two new app-reconciliation tests, because both sides share the same add/select preamble lines. |
| `6e5ed78` | insertion point in scrolling layout | TAKE | Routes app-reconciliation through the insertion policy. Orthogonal to §2.D: never reads `Column.tabbed`/`active`/`width_offset`/`tab_groups`. |

### Stage 4 — `e546861` → `4a2e3b5` · 6 commits · 6 hunks · **protocol re-homing**

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `25d5923` | rift-client library | TAKE | Cargo workspace + Mach transport extraction. Apply the nix change in the **same** commit (§4.3) or the build breaks. |
| `8f0d925` | dimmer example | TAKE | Free; excising costs more than carrying. Its `ctrlc`→`nix 0.31.3` dev-dep subtree is kept out of the nix build by the `--package` scoping in §4.3. |
| `7fc1089` | match higher max message size | TAKE | Mandatory: `MAX_MESSAGE_SIZE` 16 KiB → 256 KiB, or the server truncates typed payloads. |
| `a39581e` | rift-protocol | REWORK | Wire format changes shape: `ExecuteCommand{command: RiftCommand}`, `Subscribe{event: EventKind}`, `GetWindowInfo{window_id: WindowId}`. Our four `RiftCommand::Reactor` sites in `rift-cli.rs` become `CliCommand::Reactor`. |
| `103af15` | handling of legacy untyped clients | TAKE | Back-compat shim for the *old* wire format — i.e. ours. Skipping it breaks every stale `rift-cli` and any sketchybar/Lua consumer. |
| `e546861` | typed event data | REWORK | The big re-homing. See §4.4. **Reality:** the largest single piece of work in the sync and it went as analysed — five variants moved to `crates/rift-protocol/src/commands.rs`, `MoveFocusArgs` kept its dual-form `Deserialize`, `engine.rs` re-exports the protocol type (`pub use rift_protocol::{LayoutCommand, MoveFocusArgs}`). The `#[serde(default)]` on `MoveWindowToWorkspace.follow` was added — but for the reason in §4.6, not the predicted one. |

### Stage 5 — `upstream/main` (`6c64d8b`) → `4e8b56e` · 5 commits · 2 hunks

| Commit | Subject | Verdict | Note |
|---|---|---|---|
| `12e8efc` | roll strip automatically on focus change (#434) | TAKE | Needs `b9b79be`'s `changed`. **Not** a re-add of the off-left snap we reverted — it touches zero lines of `scrolling.rs`; upstream's offset clamp is unchanged. Keep our revert. Complementary to §2.D: it is what makes our per-column tab memory visible on non-keyboard focus paths (Dock, IPC). |
| `155d522` | mouse_hides_on_focus (#374) | TAKE | Needs `a5fa216`. |
| `b619ce5` | hot reload non-keybinding settings | REWORK | `service_config_update: Option<(Config,bool)>` → `Option<Config>`. **Certain conflict:** upstream rewrites the exact block holding our `hints_bar_tx → ConfigUpdated` arm. A careless "take theirs" silently deletes hints-bar hot reload. Bonus: `event_tap`'s cached `stack_line_*` settings finally hot-reload without a keybinding change. **Reality: the best call in this document.** The conflict landed exactly there, the arm was preserved through it, and nothing would have failed if it had not been — no test covers hints-bar hot reload. Now a permanent playbook item (`FORK.md` §4). |
| `151d99f` | rift is stable | TAKE | README wording. |
| `6c64d8b` | more app rule fields (#375) | REWORK | See §5.1 — partially delivers our roadmap item 3. Adds `Column.width_overridden`, which needs a mechanical sweep of our 21 exhaustive `Column` literals in `scrolling.rs`. **Reality:** the count was right — 21 exhaustive constructor literals, of which the **12 fork-only test literals** needed the hand sweep (upstream's own 9 came with the field). Bonus, unpredicted: upstream's new `focus = true` app-rule path is a focus grant, so it was wired to carry `activate: activate_on_focus` — a rule-driven focus now follows the same §2.B policy as every other focus path. |

**Net: 38 TAKE, 0 SKIP** — and that held: nothing upstream added turned out to be
unnecessary, and nothing upstream added superseded a local feature. **Correction:
the plan's "exactly one line of ours gets deleted as redundant" was wrong — zero
were.** The `maybe_send_menu_update` call it meant was upstream's own (see the
`fe8a687` row above).

**One local *behaviour* was dropped, which the plan did anticipate:** `5e5916f`'s
"non-tabbed stacks move as a whole". See §5.1 for the resolution as landed and
`FORK.md` §2's dropped list for why it must not come back.

---

## 4. The five pieces of real work — as landed

Everything else was mechanical. These are the ones that needed judgment. §4.6 is
the honest scorecard for the five.

### 4.1 §2.B `activate` vs upstream's activation rework — *the one semantic conflict*

Upstream does **not** foreground applications and is not moving that way.
Verified: `NSRunningApplication::activate` / `activateWithOptions` appear nowhere
in upstream's `src/actor/app.rs` or `src/sys/app.rs`; upstream's `RaiseRequest`
is still the 4-tuple `(wids, token, sequence_id, quiet)` against our 5-tuple. The
one `app.activate()` upstream added is `NSApplication::sharedApplication(mtm)` in
the Mission Control overlay — Rift activating *itself* to take key focus, not
per-app foregrounding. **§2.B stays, in full, and is not even partially
duplicated.**

`652a53f` + `7f26efe` changed the ground under it. `652a53f` makes the Carbon
front-process edge authoritative and routes it through `on_global_activation`;
`handle_application_activated` treats `Quiet::Yes` as "initiated by Rift" and
skips the auto workspace switch. Our `running_app.activate()` fired *before*
`wait_for_activation` armed `last_activated`, so the Carbon edge our own call
produces resolved to `Quiet::No` — "the user did this" — and could trigger a
spurious auto workspace switch. Two fixes landed in `69dd857`:

1. **`src/actor/app.rs`** arms `pending_activation_quiet = Some((Instant::now(),
   Quiet::Yes))` immediately **before** `let _ = this.running_app.activate()`, so
   upstream attributes the edge to Rift. `on_global_activation` consumes the
   marker within a 1 s window (upstream's own `on_ax_activation_changed` idiom),
   and `handle_frontmost_changed` clears it when the app is no longer frontmost.
2. **`src/actor/reactor.rs`** sets `response.activate = false` on the
   `WorkspaceSwitchOrigin::Auto` path — that switch *reacts* to an app already
   becoming frontmost, so re-foregrounding it is a wasted
   `ActivateIgnoringOtherApps` that can feed back into `on_global_activation`.
   Keyboard and IPC switches keep their `activate`.

Never an infinite loop (`652a53f` adds `is_globally_frontmost` dedup and drops the
"any standard window of this pid" fallback), but a real behavioural regression risk
and the merge's only genuine design decision.

**Both fixes remain untested by the suite, but were verified by hand (2026-08-08,
passed).** `cargo check` and `just test` catch neither; nothing in the curated suite
exercises the Carbon activation edge. The check that matters reproduces the hazard —
an app frontmost-able on the active workspace that *also* owns a window elsewhere:
park one window of a multi-window app on another workspace without `--follow`, then
drive `window focus <dir> --activate` repeatedly and assert the active workspace
never changes. Executed with Emacs parked on ws0 while still present on ws3: twelve
`--activate` moves, active workspace stayed `3` throughout. `--activate` also
demonstrably foregrounds (frontmost changed with the flag, not without), and the log
showed `MoveFocus(MoveFocusArgs { direction: …, activate: true })` arriving through
the new typed IPC. Full procedure in `FORK.md` §2.B.

**Expect `Quiet::No` in the log on a keyboard focus move — that is correct.**
`on_global_activation` prefers `last_activated` (armed by `wait_for_activation` with
the raise's own `quiet`) over our `pending_activation_quiet` marker, so an explicit
focus command is attributed to the user by design. Our pre-arm covers the narrower
path where `last_activated` is *not* armed (`waits_for_activation == false`, e.g. the
target app is already frontmost). Do not "fix" the `Quiet::No`.

### 4.2 ScreenCaptureKit packaging (`84ea8e8`) — landed

Mission Control's rewrite turned a pure-code change into a packaging + permissions
change for our `.app` bundle. What shipped:

- **SDK pin:** `nix/package.nix`'s shared `args.buildInputs` is now
  `[ pkgs.apple-sdk_15 (pkgs.darwinMinVersionHook "12.3") ]`. The devShell gets
  `pkgs.apple-sdk_15` too, plus an explicit `DEVELOPER_DIR = "${pkgs.apple-sdk_15}"`
  — the devShell uses a naked stdenv, so **no setup hook fires** and neither
  apple-sdk's `DEVELOPER_DIR` hook nor `darwinMinVersionHook` applies; the cc /
  bintools wrappers read `DEVELOPER_DIR` at runtime, so without it an ambient
  `cargo build` cannot find ScreenCaptureKit. Deliberately **no**
  `MACOSX_DEPLOYMENT_TARGET` in the devShell: the ambient default is already above
  the 12.3 floor and pinning it there would diverge from the crane build.
  **This pin was not the blocker it was billed as — see §4.6, correction 1.**
- **Frameworks:** no explicit `-framework` flags — the `objc2-*` crates emit
  `#[link(kind="framework")]`. SDK availability is sufficient. `strictDeps = true`
  stays valid (these are `buildInputs`).
- **Info.plist — two files, both done:** `assets/Info.plist` *and* the heredoc
  plist generated in `nix/package.nix` (the one actually shipped) now carry
  `NSScreenCaptureUsageDescription`. Without the purpose string macOS **kills the
  process** at the TCC prompt instead of showing it.
- **No entitlement exists** for screen capture — it is purely TCC-gated
  (`kTCCServiceScreenCapture`). No `.entitlements` file, and App Sandbox stays off.
- **TCC persistence:** the stable `rift-codesign` identity story carries over (same
  identity + bundle-id keying as Accessibility), so the grant survives
  `darwin-rebuild` — but Screen Recording is a **separate consent record**, so
  every existing user gets **one new prompt** at the next login, raised by a
  launchd agent, which is easy to miss. Ad-hoc (`signingIdentity = "-"`) installs
  get re-prompted on every rebuild, same as Accessibility today.
- **Hardening (ours, not upstream's), landed separately as `d1622e6`:**
  `src/ui/mission_control.rs` probes `CGPreflightScreenCaptureAccess()` once
  through a `LazyLock<bool>` (`SCREEN_CAPTURE_PERMITTED`) and fails the capture
  batch with a single clear `warn!` instead of rendering a silently blank grid.
  `FORK.md` §8 stability item 2 (a WM should degrade, not crash), partially closed.

**Verified:** `nix build .#rift-unwrapped` succeeds and both binaries run.

### 4.3 Cargo workspace → nix build scope (landed with `4a2e3b5`)

- `src` filtering needed **no change**: `craneLib.fileset.commonCargoSources` is
  recursive over `*.rs`/`*.toml`/`Cargo.lock`, so `crates/**` is captured.
- `pname`/`version` needed **no change**: the root keeps `[package] rift-wm`
  alongside `[workspace] members = [".", "crates/rift-client", "crates/rift-protocol"]`,
  so `crateNameFromCargoToml` still resolves (a virtual manifest would not have).
- `flake.nix` needed **no change** — it holds no cargo/crane knobs.
- **The one required change:** `cargoExtraArgs = "--locked --package rift-wm --bins";`
  in the **shared `args` attrset**, not just the `buildPackage` call. crane keys
  `cargoArtifacts` reuse on the argument set, so if `buildDepsOnly args` and
  `buildPackage (args // { … })` disagree on `cargoExtraArgs`, every dependency
  silently rebuilds — a slow, silent, easily-missed regression rather than an
  error. It also keeps the `rift-client` examples' `ctrlc`/`nix 0.31.3`/`dispatch2`
  dev-dependency subtree out of the build.
- `Cargo.lock`: took upstream's wholesale (our fork adds no dependencies). crane
  defaults to `--locked`, so a stale lock is a hard failure, not a silent
  re-resolve.
- `resolver = "3"` needs cargo ≥1.84; we are on `edition = "2024"` (≥1.85), so the
  pinned fenix toolchain sufficed.
- The suggested `-p rift-wm` for `justfile`'s `test` recipe was **not** applied and
  is not needed: `cargo test --lib` from the workspace root already resolves to the
  root package, and `just test` ran clean unchanged at every stage.

### 4.4 Re-homing our command variants into `rift-protocol` (`e546861`) — landed

All five now live in `crates/rift-protocol/src/commands.rs`:

| Ours | From | To |
|---|---|---|
| `LayoutCommand::MoveFocus(MoveFocusArgs)` **+ the struct + its custom `Deserialize`** | `layout_engine/engine.rs` | `crates/rift-protocol/src/commands.rs` |
| `LayoutCommand::FocusColumn(usize)` | `layout_engine/engine.rs` | same |
| `LayoutCommand::ToggleColumnTabbed` | `layout_engine/engine.rs` | same |
| `LayoutCommand::CycleColumnWidth` | `layout_engine/engine.rs` | same |
| `ReactorCommand::ToggleHintsBar` | `model/reactor.rs` | same |

They had no choice about moving: the enums that hold them left `src/`. Match arms,
clap definitions, and the `WmCmd::ToggleHintsBar` bridge all stayed in `src/`.
`rift-protocol` depends only on `serde`/`serde_json`, and
`MoveFocusArgs { direction, activate }` ported cleanly. `engine.rs` re-exports the
protocol types (`pub use rift_protocol::{LayoutCommand, MoveFocusArgs};`) so every
existing `src/` callsite kept compiling unchanged.

**The `MoveFocusArgs` landmine — still live, now one crate away.** It keeps its
dual-form `Deserialize` (`Repr::Bare(Direction)` | `Repr::Full { direction, activate }`),
because `rift.default.toml` binds the bare `move_focus = "left"` and that file is
parsed by `Config::default().unwrap()`. Upstream's `103af15` puts
`#[serde(rename = "direction")]` on its `MoveFocus(Direction)` newtype — that
attribute occupies exactly the slot our struct replaces. Keeping upstream's
`MoveFocus(Direction)` while `engine.rs` destructures
`MoveFocusArgs { direction, activate }` gives `E0308`; porting the struct but
dropping `Repr::Bare` gives a **runtime panic in `Config::default()` that
`cargo check` cannot see**. Ours is a strict superset of upstream's and stays.

**This trap is now guarded.** `common::config::default_config_parses` (`1c73bb3`)
parses the embedded `rift.default.toml` and asserts the four bare `move_focus`
binds survive, so dropping `Repr::Bare` fails a *named* test instead of aborting
the whole test binary with no attribution.

**`activate` is now public wire API.** Once `MoveFocusArgs` lives in
`rift-protocol`, our `activate` flag is part of the *public* IPC vocabulary, and
`103af15`'s legacy decoder routes old `{"Reactor":{"move_focus":"left"}}` payloads
through the same `Deserialize` — one impl, two back-compat duties (default-config
parsing *and* legacy IPC). Good outcome, but it means we own `activate`'s
compatibility: `Repr::Bare` must keep defaulting it to `false`. Proven at runtime
during stage 4, since `cargo check` cannot: `{"move_focus":"left"}` →
`activate: false`, the table form → `activate: true`, `follow` absent → `false`.

`MoveWindowToWorkspace.follow` did get `#[serde(default)]`, but not for the
predicted reason — see §4.6, correction 2.

### 4.5 Mechanical sweeps — predicted vs actual

| Upstream change | Our callsites (predicted) | Actual |
|---|---|---|
| `EventResponse` gains `changed: bool` | ~22 literals in `layout_engine/engine.rs` (union with `activate`); destructure in `reactor.rs` needs `changed: _`; two literals in `reactor/tests.rs` need `changed: true` | **Wildly over-estimated.** git auto-merged nearly all of them; exactly one hand edit — our fork-only `FocusColumn` handler needed `changed: true`. §4.6, correction 3 |
| `Column` gains `width_overridden: bool` | **21** exhaustive `Column { .. }` literals in `systems/scrolling.rs` (ours also carry `tabbed` + `active`) | Count exact: 21 constructor literals, of which the **12 fork-only test literals** needed the hand sweep. Fork-only lines get no auto-merge help |
| `space_scope: Option<SpaceId>` on `update_layout`/`calculate_layout`/`update_layout_or_warn(_with)` | `managers.rs` (incl. the reserve-convergence recursion), 3 `reactor.rs` sites, 1 test | As predicted. Threaded through unchanged; see `FORK.md` §2.E |
| `service_config_update: Option<(Config,bool)>` → `Option<Config>` | `events/outcome.rs` (4 sites), `events/command.rs`, `reactor.rs` destructure | As predicted, and the `hints_bar_tx` arm survived — the point of the whole prediction |
| `ReactorCommand::FocusWindow.window_id` → `rift_protocol::WindowId` | 2 sites in `actor/hints_bar.rs` (chip-click + detail overlay) — add `.into()` | As predicted; matches upstream's own conversion in `stack_line.rs` |
| `MissionControlActor::new` gains a `Sender` | 1 site in `bin/rift.rs` | As predicted |
| `Animation::new()` → `Animation::new(Config)` | upstream-internal only | Confirmed |
| `CgsWindow::bind_to_context` → `bind_layer_context` (unsafe) | **grep `ui/stack_line.rs` + `ui/hints_bar.rs` before merging** — both own CGS windows | No fork-side change needed |
| `window_server::{CapturedWindowImage, capture_window_image, resize_cgimage_fit}` **removed** | grep before merging | No fork callers |
| `MainWindowTracker::take_global_activation_quiet` → `is_globally_frontmost` | 1 site in `reactor.rs` | As predicted |
| `EventOutcome::{finalized_event, activate_application, with_application_activation}` **removed** | delete our copies; no local callers | As predicted |
| `layout_engine.rs` re-export | 3-way union: our `MoveFocusArgs` + upstream's `LayoutEventOutcome` + `Restore*` | As predicted; resolved in stage 5 |

`just test`'s allowlist survived intact — all five module-path filters
(`layout_engine`, `actor::raise_manager`, `actor::reactor::tests`,
`common::config`, `ui::stack_line`) still resolve, and the known-broken
`topology_change_clears_stale_pending_hide_target_*` still exists upstream, so the
`--skip` stays.

**Gap found while checking that, since CLOSED (2026-08-08):** the allowlist had no
`ui::hints_bar` filter, so the §2.E hint-bar geometry/click tests **never ran under
`just test`** despite `FORK.md` §2.E claiming they did. Not caused by this sync. The
filter was added when the `justfile` was next touched, taking the suite from 353 to
359 tests. See §4.7.

### 4.6 Predictions that were wrong

The most useful section here for the next maintainer. **Seven** calls were wrong —
five predicted in this document (below) and two found during live testing (§4.7) —
and the pattern across them is worth more than the individual facts: every one was
an assumption never checked against the thing it described — the pinned nixpkgs, the
enum that actually deserializes, the diff git would produce, the rr-cache hit rate,
the authorship of a line, the config flag, and the verification command itself.

**1. §4.2 called the SDK pin "build-breaking". It was not a blocker at all.**
The claim was that `clangStdenv` inherits apple-sdk **11.x**, so `0bf5549`'s
ScreenCaptureKit link would fail without an explicit pin. Reality: the *pinned*
nixpkgs defaults to apple-sdk **14.4** with `darwinMinVersion` **14.0** — already
well past SCK's 12.3 floor. `0bf5549` would have linked with `buildInputs = [ ]`
untouched. The explicit `pkgs.apple-sdk_15` + `darwinMinVersionHook "12.3"` pin is
still the right change (it makes the floor explicit and buys immunity from a
nixpkgs default bump), but it was insurance, and sequencing stage 3 behind it was
wasted caution. `darwinMinVersionHook` is a *floor*: it raises
`MACOSX_DEPLOYMENT_TARGET` only if lower, so at 14.0 it is a no-op.
**Lesson: read the default out of the nixpkgs you actually pin, not out of memory.**

**2. §4.4's "second `Config::default()` panic" did not exist.**
The claim was that `MoveWindowToWorkspace.follow` losing `#[serde(default)]` in the
crate move was a startup panic of the same class as `MoveFocusArgs`. It is not:
bare `move_window_to_workspace = N` binds never reach `LayoutCommand` — they
resolve through `WmCmd::MoveWindowToWorkspace(WorkspaceSelector)` under
`#[serde(untagged)] WmCommand`. Upstream's own default TOML uses the bare form and
does not panic. `#[serde(default)]` was still added, for a different and real
reason: upstream's move dropped the `default = "crate::common::config::no"` the
field carried in `src/` (a protocol crate cannot reference `crate::`), which would
have made the documented *table* form `{ workspace = N, window_id = 123 }` a parse
error and broken legacy IPC clients that omit `follow`. Right change, wrong stated
reason — and a wrong reason is how a future maintainer "fixes" it back out.
**Lesson: with `#[serde(untagged)]`, the panic surface is the enum that actually
deserializes the value, not the enum that owns the type.**

**3. "~22 `EventResponse` literals to union by hand" was off by ~22.**
git auto-merged effectively all of them. The reason is mechanical and predictable
in hindsight: upstream's `changed` and our `activate` sit at opposite ends of the
struct, so the two sides edited non-adjacent lines and there was nothing to
conflict. Exactly one literal needed hand work — our fork-only `FocusColumn`
handler, which has no upstream counterpart to inherit `changed` from and so needed
a manual `changed: true`. The same rule explains why `Column.width_overridden`
*did* cost real work: those 12 literals are **fork-only test code**, lines upstream
never touched, so there was no upstream-side edit to merge in and every one had to
be swept by hand.
**Lesson: a compiler-enumerated field addition is not a conflict estimate. Cost
tracks how much of the affected code is fork-only, not how many callsites exist.**
Estimate the next sweep by counting *fork-only* literals.

**4. `rerere` was useless, and the caveat about it was too soft.**
All 3 cached resolutions missed every hunk — a 0% hit rate, not a degraded one.
`.git/rr-cache` now holds 12 fresh resolutions from the five stages (plus 4 from
the exploratory probe), which will help *only* if the surrounding code has not
moved by the next sync. It is also **local-only and uncommitted**, so it protects
this machine and nobody else. Treat rerere as a convenience for back-to-back
merges, never as a plan. §7.

**5. `fe8a687`'s `DROP-OURS` verdict misattributed an upstream line to us.**
§3 labelled it "TAKE + **DROP-OURS** — delete *our* duplicate
`maybe_send_menu_update()` in `managers.rs`", and §3's summary claimed "exactly one
line of ours gets deleted as redundant". Both wrong: that call was **upstream's**.
Verified after the fact — `fe8a687^`'s `managers.rs` contains it, `fe8a687`'s does
not, and the commit relocates it into `reactor.rs`. Zero fork lines were deleted in
this sync. The misattribution is the interesting part: the line sat next to our
local `managers.rs` edits, so it showed up on "our" side of a conflict hunk and got
read as a fork feature.
**Lesson: a line appearing on your side of a conflict does not make it yours. Before
labelling anything `DROP-OURS`, confirm with `git log -S'<symbol>' upstream/main --`
or `git show <upstream-commit>^:<path>`.** Cheap check, and getting it backwards is
how you either delete a real feature or "restore" code you never wrote.

**What was right and is worth repeating:** the chronological-checkpoint decision
(§2), the `b619ce5` hints-bar-arm hazard (`FORK.md` §4 now carries it permanently),
the `MoveFocusArgs`/`Repr::Bare` landmine, and the call that `652a53f` was the
highest-risk commit in the backlog. Predicted conflict surface (8 files, 16 hunks)
matched the executed total exactly.

### 4.7 What the live pass found (2026-08-08)

The merges were gated on `cargo check` + `just test`, which is why both of these
survived to `just install`. Neither is a merge defect; both are the *gates* being
narrower than they looked.

**6. "Mission Control is broken, and it's upstream's bug." Wrong — it is off by
default.** During live testing the overlay never appeared: the command reached the
reactor, `wm_controller` forwarded it, no overlay, no capture, no actor log lines. I
diffed the whole path against upstream (`actor/mission_control.rs` byte-identical;
our only diffs the additive `WmCmd::ToggleHintsBar` and the inert §2.F preflight),
concluded upstream regression, and leaned on the commit being titled
"improvements to mission control **1/n**" as supporting evidence. All of it was
built on not reading the config: `rift.default.toml` ships
`[settings.ui.mission_control] enabled = false`, and `MissionControlActor::run()`
gates on that flag *inside* its receive loop, so every event is silently dropped.
Enabled, it works — live ScreenCaptureKit previews, legible contents, focus border.
Two compounding errors worth naming: (a) I treated "zero log lines from the actor"
as evidence of a broken actor when the actor simply has no logging on that path, and
(b) I read a commit-message convention as evidence of a code state — "1/n" is the
only such commit in the repo's entire history, so there was no pattern to infer
from. Triage order now documented in `FORK.md` §2.F.
**Lesson: for "feature does nothing", check the feature flag before the code. A
silent no-op is far more often a gate than a bug — and never let a commit message
substitute for reading the config.**

**7. `just fmt-check` was itself broken, which hid 47 real hunks.**
`just fmt` / `fmt-check` / `clippy` were bare `cargo …` with only a *comment*
instructing the caller to be inside `nix develop` — unlike `just test`, which always
enforced it. Outside the dev shell they resolved to system **stable** rustfmt
(1.8.0-stable), which parses `rustfmt.toml`, warns that the unstable options need
nightly, ignores them, and reports the whole tree as drift: **411 phantom hunks**.
That number is also what made `FORK.md`'s "baseline drift is pre-existing, do not
reformat" note look plausible — and that note was false. With the flake-pinned
nightly (1.8.0-nightly): pristine `upstream/main` is **completely clean**, and our
tree had **47 real hunks**, all in fork-owned files. Fixed in `54ce476` (recipes now
`nix develop -c`) and `9f26883` (the 47 hunks). `just fmt-check` is now 0.
**Lesson: a verification command that cannot fail correctly is worse than none — it
manufactures noise, and someone then documents the noise as expected. When a check
reports implausible output, verify the check before believing or excusing it.**

Also closed by this pass: the `ui::hints_bar` allowlist hole flagged in §4.5. The
filter was added, taking `just test` from 353 to 359 — §2.E's six geometry/click
tests had never run. Same failure mode as #7: a check that silently covered less
than it claimed.

---

## 5. Where we stand relative to upstream after the sync

### 5.1 Our features vs upstream

| Ours | Status after sync |
|---|---|
| §2.A nix packaging | Fork-only. Upstream still ships no Nix support. Gained the §4.2 SDK/permissions work and the §4.3 workspace scoping; both landed. |
| §2.B app activation on focus | **Fork-only and unique.** Upstream deliberately never foregrounds apps — re-verified against the merged tip: zero `NSRunningApplication::activate` / `activateWithOptions` in upstream's `src/actor/app.rs` or `src/sys/app.rs`, and upstream's `RaiseRequest` is still a 4-tuple against our 5-tuple. §4.1 rework landed. |
| §2.C stack-line tabbed titles | Fork-only. No upstream equivalent. |
| §2.D niri tabbed columns (scrolling) | **Fork-only, still ahead** — but no longer strictly a superset: the non-tabbed horizontal-move behaviour is now upstream's (see below). `Column` now also carries upstream's `width_overridden`. `12e8efc` and `6e5ed78` remain complementary — they make our tab memory *more* visible, not redundant. |
| §2.E hint bar | Fork-only. Hot reload survived `b619ce5` (the predicted silent-deletion hazard); the reserve-convergence recursion now threads `space_scope`. |
| Per-column width presets (`76195ff`) | Fork-only. Roadmap item 2, still ours. |
| Panic hygiene (`c32893b`) | **Ours is better and was defended.** Upstream still has `dirs::home_dir().unwrap()` in `config.rs` (4 sites at the merged tip); our non-panicking `home_dir()` helper was on the *our* side of a stage-4 conflict hunk and was kept. Do not "take theirs" there — ever. |
| Property tests (`46a983a`), raise_manager hardening (`4d97013`) | Fork-only. Green through all five stages. |

**One genuine design disagreement — resolved, with a deliberate behaviour change.**
Upstream `95bc739`-era `scrolling.rs` makes a horizontal `move_node` on a
multi-window column **extract** the focused window into its own neighbour column
("a faster way to undo accidental stacks"). Our `dcc768d` + `5e5916f` did the
opposite — a column moves *as a whole*, and you use consume/expel to move windows
in or out.

**Landed resolution: split the behaviour on `Column.tabbed`.**

- **Tabbed column → moves as a whole.** Ours (`dcc768d`), kept: a tab group *is*
  one unit by definition. Test `horizontal_move_shifts_tabbed_column_as_a_whole`
  kept unchanged.
- **Plain (non-tabbed) vertical stack → extracts.** Upstream's, adopted.
  **`5e5916f`'s behaviour is dropped** and is now on `FORK.md` §2's
  "Dropped / superseded (do NOT re-add)" list. Extraction also clears the source
  column's `active` tab memory, so it cannot point at a window that no longer
  lives there.

Tests, as they now stand in `systems/scrolling.rs`:

| Before | After |
|---|---|
| `horizontal_move_shifts_tabbed_column_as_a_whole` | unchanged — still asserts whole-column movement |
| `horizontal_move_shifts_non_tabbed_stack_as_a_whole` | **replaced** by `horizontal_move_extracts_selected_window_from_a_non_tabbed_stack` |
| `move_selection_does_not_extract_from_lone_stacked_column` | **replaced** by `move_selection_right_extracts_from_a_lone_stacked_column` |
| `move_selection_left_does_not_extract_from_lone_stacked_column` | **replaced** by `move_selection_left_extracts_from_a_lone_stacked_column` |

Why drop ours rather than defend it: the behaviour lives in `scrolling.rs`,
upstream's most-churned file, and upstream's version is deliberate and defensible.
Carrying a bare *preference* there costs a conflict every single sync. Extraction
is also the more useful default — it is the fast way to undo an accidental stack,
and `toggle_column_tabbed` recovers whole-column movement whenever you want it.

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

**Stability roadmap item 1 is now partly closed.** The claim in `FORK.md` §8 that
`default_config_parses()` existed was **false when written** — the test did not
exist anywhere in the repo. It exists now: `common::config::default_config_parses`
(`1c73bb3`) parses the embedded `rift.default.toml`, asserts the keymap is
non-empty, and asserts the four bare `move_focus` binds still resolve through
`MoveFocusArgs`'s `Repr::Bare` arm. Verified passing as
`common::config::tests::default_config_parses`; it fails *by construction* if
`Repr::Bare` is dropped, because the bare bind then stops deserializing and
`Config::parse(..).expect(..)` panics. It is inside `just test`'s `common::config`
filter, so it actually runs.

That was the cheapest high-value item on the list, and this sync proved why:
`95bc739` puts `#[serde(flatten)]` on `deny_unknown_fields` structs and §4.4 moved
`MoveFocusArgs` across a crate boundary — both silent-until-runtime.

**The rest of item 1 is still open.** Upstream's test overhaul does not help:
`Cargo.toml` is untouched by it (`panic = "unwind"` was already set), the
`failed to initiate panic` abort remains — **we hit it three times during this
merge**, each time losing the whole run to one failure with no attribution — no
window-server seam was added, and `nix flake check` is still unwired.

---

## 6. Outcome log (was: ordered plan)

What each stage actually cost. Conflict counts are hunks resolved by hand; every
stage was gated on `cargo check` (`--workspace --all-targets` from stage 4 on) plus
`just test` before its merge commit was finalized.

**Stage 1 — `ccfcc45` → `84382a7`. 13 commits, 0 conflicts, 327/327.**
Nothing to decide, exactly as predicted. Pre-executed in a throwaway worktree
first, then landed. A third of the backlog, free.

**Stage 2 — `b6deef3` → `69dd857`. 9 commits, 6 hunks, 343/343 (+16).**
The hard one, correctly identified.
- `EventResponse`: unioned upstream's `changed` with our `activate`. Only the
  fork-only `FocusColumn` handler needed a hand edit (`changed: true`) — the
  predicted ~22-literal sweep never materialized (§4.6).
- `activate_workspace`: kept our hoisted `focus_window` local (used twice) rather
  than upstream's inlined call.
- `managers.rs`: threaded `space_scope` through the hints-bar reserve-convergence
  recursion unchanged, so a scoped workspace switch does not widen to a full
  re-layout when the bar re-reserves.
- Deleted the redundant `maybe_send_menu_update` call in `managers.rs` per
  `fe8a687`. Not a fork line, despite the plan's `DROP-OURS` label — see §3.
- `config.rs` / `rift.default.toml`: unioned our hints-bar + external-bar keys with
  upstream's `BaseLayoutSettings`/Traditional/Bsp tables. No name collisions.
- **§4.1 activation rework**, the one semantic conflict: `app.rs` pre-arms
  `pending_activation_quiet = Quiet::Yes` before `running_app.activate()`;
  `reactor.rs` clears `response.activate` on the `WorkspaceSwitchOrigin::Auto`
  path. Both runtime-only — no test covers them; verified by hand.

**Stage 3 — `6e5ed78` → `e270ce2`. 5 commits, 2 hunks, 345/345 (+2).**
The packaging (`84ea8e8`) went in first as planned — unnecessarily, as it turned
out (§4.6). The real cost was somewhere the plan never looked: git interleaved our
`scrolling.rs` test tail with upstream's two new app-reconciliation tests, because
both sides share the same add/select preamble lines. **Resolved by reconstructing
the region from both blobs** (`git show HEAD:<path>` / `git show <theirs>:<path>`)
rather than doing surgery on the conflict markers — our four tests + proptest
block, then upstream's two. All six present and passing. This technique is now a
`FORK.md` §4 playbook item; the same interleave recurred in stage 5.

**Stage 4 — `e546861` → `4a2e3b5`. 6 commits, 6 hunks, 345/345.**
The largest piece of work. Five command variants re-homed into
`crates/rift-protocol/src/commands.rs`, `MoveFocusArgs` + its dual-form
`Deserialize` moved with them, `engine.rs` re-exports the protocol types. Match
arms, clap defs and the `WmCmd` bridge stayed in `src/`. `#[serde(default)]` added
to `MoveWindowToWorkspace.follow` (right change, wrong predicted reason — §4.6).
`hints_bar.rs` gained `.into()` at both chip-click routes for
`rift_protocol::WindowId`. Our non-panicking `home_dir()` was defended in
`config.rs`. **Verified beyond the gates:** `nix build .#rift-unwrapped` succeeds
with the new workspace scoping, both binaries run, the four fork CLI verbs
(`toggle-tabbed`, `cycle-column-width`, `focus-column`, `--activate`) are still
present, and the dual-form parse was proven at runtime because `cargo check`
cannot see it.

**Stage 5 — `upstream/main` (`6c64d8b`) → `4e8b56e`. 5 commits, 2 hunks, 352/352 (+7).**
- `layout_engine.rs` re-export: 3-way union — our `MoveFocusArgs` plus upstream's
  `LayoutEventOutcome` and the `Restore*` set.
- `Column` gained `width_overridden`; swept the 12 fork-only test literals.
- **The `b619ce5` hazard was real and the prediction paid for itself:** upstream
  rewrote the exact block holding our `hints_bar_tx → ConfigUpdated` arm. The arm
  was preserved; `service_config_update` is now `Option<Config>`. Nothing would
  have failed if it had been dropped — no test covers hints-bar hot reload.
- Upstream's new app-rule `focus = true` path now carries
  `activate: activate_on_focus`, so a rule-driven focus grant follows the same
  §2.B policy as every other focus path.
- The §5.1 horizontal-move split landed here, with its three test rewrites.

**Cleanup, after the merges.** `1c73bb3` added `default_config_parses`
(`FORK.md` §8 stability item 1, previously claimed-but-absent). `d1622e6` added the
Mission Control Screen-Recording preflight (`FORK.md` §8 stability item 2).
`FORK.md` was updated: branch `xieyt/4` → `xieyt/5`, §2.A packaging, §2.B
activation attribution, §2.D the tabbed split, §2's dropped list, §2.E the
`b619ce5` hazard, and a rewritten §4 playbook. `.git/rr-cache` holds 12 fresh
resolutions — **local-only and uncommitted**, see §7.

---

## 7. Keeping current after this

The 38-commit backlog cost this much analysis because we let it accumulate. The
numbers, now measured rather than predicted:

| | Commits | Hand-resolved hunks |
|---|---|---|
| Stage 1 (no structural change) | 13 | **0** |
| Stages 2–5 (every structural change in the backlog) | 25 | 16 |

**Two thirds of the hunks came from three commits** — the workspace conversion
(`25d5923`/`e546861`), the `EventResponse.changed` field (`b9b79be`), and the
hot-reload rewrite (`b619ce5`). Everything else was free or nearly so. Upstream
commits are individually small and mostly non-overlapping with our quarantined
features; it is the *structural* commits that cost, and those cost the same
whether you take them early or late — except that taking them late means resolving
them against a larger local delta *and* against surrounding code that has since
moved, which is precisely what made `rerere` useless here (§4.6, correction 4).

So: **sync on structural change, not on a calendar.** A commit count is the wrong
trigger — 13 commits cost nothing, one commit cost six hunks. Concretely:

```bash
git fetch upstream
git log --oneline HEAD..upstream/main
# structural tripwires — any hit means sync now, not later:
git diff --stat HEAD..upstream/main -- Cargo.toml Cargo.lock crates/
git log -p HEAD..upstream/main -- src/model/reactor.rs src/actor/reactor.rs \
  | grep -E 'struct EventResponse|struct EventOutcome|^\+.*fn update_layout'
git log --oneline HEAD..upstream/main -- src/layout_engine/systems/scrolling.rs
```

Sync immediately when a commit: adds or moves a crate, touches `Cargo.toml`'s
`[workspace]`, adds a field to `EventResponse` / `EventOutcome` / `Column`, changes
an `update_layout`/`calculate_layout` signature, or rewrites a block in
`reactor.rs`'s outcome fan-out. Otherwise a monthly `git merge upstream/main` is
fine — a pile of non-structural commits is genuinely cheap, and this sync is the
evidence.

**And do it in checkpoints regardless of size.** `git merge <sha>` per chronological
checkpoint, each gated on `cargo check --workspace --all-targets` + `just test`,
never one 38-commit jump: the dependencies between upstream commits are respected
by construction, each stage's test delta is attributable, and each stage's
resolutions land in `rerere` separately.

**Fix the rerere gap.** `.git/rr-cache` is local-only and uncommitted, so its 12
fresh resolutions protect exactly one machine and vanish with the clone. Until that
changes, **§4 of this file is the durable resolution store** — when you resolve
something non-obvious, write it into `FORK.md` §4's playbook rather than trusting
the cache to replay it.
