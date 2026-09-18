# Upstream sync — 2026-08 · completed record

Companion to `FORK.md`. `FORK.md` says *what we carry and why*; this says *how we
absorbed the 38 upstream commits we were behind*, commit by commit, with the
verdict each one got and what it actually cost.

Written as a plan on 2026-08-05 and executed the same day; §8 records the follow-up
sync on 2026-08-10. Kept as the reference for the **next** sync, not as history for
its own sake:

- **§3** — why each of the 38 commits was taken. Unchanged verdicts; notes now
  say where the predicted rework differed from the real one.
- **§4** — the five pieces of real work, as landed. **§4.6 is the predictions that
  were wrong; §4.7 is what live testing found afterwards.** Read both before
  trusting anything else here.
- **§6** — per-stage outcome log (conflicts, resolutions, gates).
- **§7** — the sync cadence the measurements support. **§8.4 is that rule tested
  once, and the three `justfile` gaps closing it exposed** — including a `just sync`
  that implemented the strategy §3 explicitly rejected.
- **§8** — the 2026-08-10 sync (11 commits, 2 hunks). Start here if you are about
  to sync: it is the shortest complete worked example, and §8.3 covers what to do
  when upstream ships a failing test.

## 0. Status — five syncs complete; §11 is the newest

```
branch                    xieyt/5
behind upstream           0        (git rev-list --left-right --count HEAD...upstream/main)
ahead                     78
upstream tip merged       83f3b66  (2026-09-17; backlog ended 6c64d8b, then 1de4d09,
                                   then 7829780, then 3a52122, then 74ce00d, now this)
curated tests             327 -> 352 (backlog merge) -> 359 (allowlist fix)
                              -> 363 (§8) -> 370 (§9) -> 520 (guard widened)
                              -> 549 (§10) -> 605 (§11), plus 14 crate tests
                              that had never run at all before §11
cargo check               --workspace --all-targets clean
just fmt-check            clean
nix build .#rift          succeeds; `nix eval .#rift.version` reads 0.5.9
```

**§11 records the 2026-09-18 sync** (77 commits, 13 hunks, 2 deleted files we had
edited, 2 SKIPs) and is the one to read if you are about to sync. It is the first
of these where the cost was a **migration, not conflict resolution**: upstream
folded `event_tap.rs` + `gesture_tap.rs` — both carrying fork hunks — into a single
`input.rs`. §11.4 is the hazard register (twelve clean merges that lose a fork
behaviour); **§11.10 scores the plan's predictions, including the one that was
wrong by a whole workday in the cheap direction.**

**Upstream `main` currently has ONE failing test of its own**, reproduced on a
pristine worktree and skipped in `just test` with attribution: see §8.3.
There were three. §10 found one of the skips named a test upstream had already
renamed (a silent no-op filter); §11 found the second had been *repaired* upstream
by `e4aa60d` and deleted it. Skips are verified by deletion, never by belief.

**§10 records the 2026-08-28 sync** (22 commits, 6 hunks, 0 SKIPs) and is the one to
read for the failure mode this series had not yet hit: two fork features nearly lost
through hunks git merged *cleanly*. §10.2 is the first time we retired a fork
feature as superseded rather than defending it.

**§8 and §9 record the 2026-08-10 syncs** (11 commits, then 3 more the same day).
§8 is the first run under §7's cadence rule and the test of whether it works
(§8.4); §9 is why you read the linked issue, not just the commit messages (§9.1).

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

- **SDK pin:** `nix/package.nix`'s shared `args.buildInputs` is
  `[ pkgs.apple-sdk_14 ]`. The devShell gets `pkgs.apple-sdk_14` too, plus an
  explicit `DEVELOPER_DIR = "${pkgs.apple-sdk_14}"` — the devShell uses a naked
  stdenv, so **no setup hook fires**, apple-sdk's `DEVELOPER_DIR` hook included;
  the cc / bintools wrappers read `DEVELOPER_DIR` at runtime, so without it an
  ambient `cargo build` cannot find ScreenCaptureKit.
  **This pin was not the blocker it was billed as — see §4.6, correction 1.**
  **Downgraded `_15` → `_14` (2026-09-18, flake download audit).** 14.4 is what
  the pinned nixpkgs' stdenv already defaults to, so the clang/bintools wrappers
  reference that store path regardless: the pin is now free, while `_15` had been
  adding a whole second SDK (31 MiB fetch, 459 MiB unpacked) to the crane build,
  the devShell, and therefore to `.direnv`'s GC root. Every SCK symbol this tree
  touches is ≤ 14.0 (`SCShareableContent`, `SCContentFilter`,
  `SCStreamConfiguration`, `SCScreenshotManager`, `SCCaptureResolutionType`), and
  `apple-sdk_12` no longer exists in nixpkgs (removed in 25.11: "Nixpkgs no longer
  supports macOS 12"), so 14.4 is the floor available anyway. Verified by building
  both paths: `nix develop -c cargo build` and `nix build .#rift` both succeed,
  and both binaries link `/System/Library/Frameworks/ScreenCaptureKit.framework`
  plus `libiconv.2.dylib`.
- **`darwinMinVersionHook "12.3"` is GONE (2026-09-18, same audit), and 14.0 is
  now declared.** The hook only *raises* `MACOSX_DEPLOYMENT_TARGET`, and nixpkgs
  sets `darwinMinVersion = "14.0"` for aarch64-darwin
  (`lib/systems/default.nix:364`), so it never fired — the shipped binary has
  always been `LC_BUILD_VERSION minos 14.0`, not 12.3. It asserted a floor the
  build does not honour; §4.2's original claim that the devShell's "ambient
  default is already above the 12.3 floor" was true of the cc wrapper and
  irrelevant to rustc. Replaced by one `minOsVersion =
  pkgs.stdenv.hostPlatform.darwinMinVersion` binding wired into three places that
  must agree: the crane build (implicitly, via stdenv), the bundle plist's new
  `LSMinimumSystemVersion` (there was **none** before — the `.app` advertised no
  minimum OS), and the devShell's `MACOSX_DEPLOYMENT_TARGET`. That last one is
  load-bearing: rustc ignores the cc wrapper's `-mmacosx-version-min` and stamps
  its own `aarch64-apple-darwin` default of 11.0 when the variable is unset
  (`rustc --print deployment-target`), so devShell binaries claimed macOS 11 while
  the packaged one claimed 14. **Verified:** in the shell
  `MACOSX_DEPLOYMENT_TARGET=14.0` and `rustc --print deployment-target` agree,
  `otool -l target/debug/rift-cli` → `minos 14.0`, and the packaged
  `Rift.app/Contents/MacOS/rift` → `minos 14.0` with
  `LSMinimumSystemVersion = 14.0` in its plist. No "built for newer macOS version
  than being linked" warnings, since the target equals nixpkgs' dylib target
  instead of undercutting it.
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
**Followed through on 2026-09-18 (flake download audit):** "still the right
change" only half survived. The *SDK* pin stays but moved `_15` → `_14`, since
14.4 is the default and pinning anything else buys a second SDK in the closure
for nothing. The *hook* was deleted: a no-op that asserts a floor the build never
applies is not insurance, it is a false claim in the source — and it hid the fact
that rift's real requirement is **macOS 14.0**, now declared in the bundle plist
(`LSMinimumSystemVersion`, previously absent entirely) and mirrored into the
devShell's `MACOSX_DEPLOYMENT_TARGET`. Same lesson, one step further: after
reading the default out of the nixpkgs you pin, check what the *artifact* ends up
stamped with — `otool -l` was the thing nobody ran for a year.

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

- **The event surface is not a cheaper hint-bar feed — but the old reason for that
  is dead.** This bullet used to reject `5c0b38d`'s focused-window broadcast
  because the payload was "geometry-free" and fired "on a strict subset" of the
  events the bar cares about. **Both claims stopped being true in the fifth sync**:
  `487168d` puts per-node pixel `frame`s in `container_tree` (root = tiling area,
  columns = union of window target frames), `7e76bdb` adds stable `node_id`s, and
  `LayoutChanged` now fires on every `layout_changed` outcome — scroll, resize,
  window-open-in-an-unfocused-column included.
  Keep §2.E's per-layout-apply feed anyway, for four reasons that survive:
  (a) `LayoutStateData` carries no `WindowMember` title, so `show_titles` and the
  hover pill would need a `GetWindows` join per event; (b) the workspace rail needs
  occupancy across *all* workspaces and the event carries only the active one;
  (c) nothing in upstream's tree distinguishes a tabbed column from a plain stack
  (`role:"column", layout_kind:Vertical` either way — the same blindness `e533c69`
  recorded); (d) mach + per-subscriber `serde_json` delivery would throw away the
  `ee2abe4` dedup and coalescing layers. Take the events for external consumers;
  the one real simplification available is sourcing the feed's frames from
  upstream's new `LayoutEngine::calculate_workspace_layout` seam.
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

---

## 8. Second sync — 2026-08-10, `1de4d09` → `7829780` (11 commits)

```
behind before        11        ahead 47
conflict hunks       2         (both additive, both in §4's documented pattern)
hand-resolved files  2         src/actor/app.rs, src/common/config.rs
merge commit         5030a4c
curated tests        359 -> 363
```

### 8.1 Verdicts

| Commit | Subject | Verdict |
|---|---|---|
| `9fea58a` | chore: beautify menubar | TAKE — `src/ui/menu_bar.rs` only; we own nothing there |
| `1e3a898` | fix: make ax destroy not be authoritative (#440) | TAKE — but ships a red test, see §8.3 |
| `8f882fc` | chore: titlechanged on window elem | TAKE — verified live, see §8.2 |
| `261a980` | feat: unstripped release profile | TAKE — inert: adds `[profile.profiling]`, does not touch `[profile.release]`, and the nix build selects `release` |
| `2484722` | fix: this didnt actually do anything | TAKE — deletes an unbalanced `SLSDisableUpdate`/`SLSReenableUpdate` pair that was never a real optimization |
| `6eff0b8` | perf: optimized enhancedui handling | TAKE — the one conflict; see below |
| `02f16c3` | perf: fast path for frame ack | TAKE — drops a synchronous WindowServer `get_mouse_state()` from every frame event |
| `abdbcc8` | perf: only rebuild menu bar once | TAKE — also removes a `read_dir` from the menu update path |
| `b2c26bb` | fix: make propagation more aggressive | TAKE the constant; see §8.2 |
| `1b69d8e` | perf: eliminate redundant work from mouse/gesture hot paths | net-zero pair ↓ |
| `7829780` | Revert of `1b69d8e` | Upstream reverted its own commit two days later |

**The `1b69d8e`/`7829780` pair is why you merge in chronological checkpoints and do
not cherry-pick.** `1b69d8e` appends a `FocusConfirmation` field to `Request::Raise`
and `struct RaiseRequest` — head-on with our §2.B `activate` 5-tuple — and `7829780`
takes it back out. Merging `upstream/main` as a range absorbs both and lands exactly
where we started; nothing survived in-tree (verified: no `FocusConfirmation`,
`MOUSE_MOVE_INTERVAL_`, `resolve_mouse_window_with`, or `is_scrolling_mode_at_event`
anywhere in `src/`). Picking commits individually would have cost a 6-tuple
migration and then a revert of it, for zero net change.

### 8.2 The two conflicts, and the one live decision

**`src/actor/app.rs` — `6eff0b8`.** Upstream's new `impl Request {
disables_enhanced_ui() }` inserts into the gap *between* our two fork-modified
lines (`Request::Raise(.., Quiet, bool)` and `struct RaiseRequest(.., Quiet,
bool)`). Purely additive: take their block, keep both 5-tuples. This is §4.1's
pattern with no new twist. `6eff0b8` also moves EnhancedUI suppression from a
per-call wrapper to a refcounted `EnhancedUi` on `State` — worth knowing because
correctness now depends on `acquire`/`release` balancing across async boundaries,
which is why upstream also added `restore_if_needed` to `Drop for State`.

**`src/common/config.rs` — `b2c26bb`.** Upstream lowered
`default_overscroll_threshold()` from `0.625` to `0.15`; our `default_scroll_speed()`
sits on the next line. Took theirs, kept ours. This looks like a scrolling-feel
change and is not, which is the only reason it was safe to take blind:
`workspace_switch_threshold` has exactly one reader (`scroll_by_delta`), the scroll
offset is clamped and stored *before* the threshold branch, and the threshold's only
effect is the `Option<Direction>` that becomes `EventResponse::boundary_hit` —
consumed at one site gated on `propagate_to_workspace_swipe`, which is `false` in
both `dev-config.toml` and the shipped `rift.default.toml`. Pixel offsets are
bit-identical either way.

**`8f882fc` needed a live check, not a code read.** It moves `TitleChanged` from
`APP_NOTIFICATIONS` to `WINDOW_NOTIFICATIONS`. That is a real hazard, because
`register_window_notifications` returns `false` on the *first* notification that
fails and `register_window` then returns `None` — so if any window's AX element
rejected `kAXTitleChangedNotification`, that window would silently stop being
tracked and tiled. Verified on the live desktop instead of assumed: 8 windows across
6 apps still tracked after install, and titles still update live (a controlled
generator emitting OSC-0 sequences read back through `rift-cli query windows` as
`TT-7 → TT-29 → TT-50 → TT-72`). No regression.

> Measuring that needs care: `alacritty --title X` **locks** the title and ignores
> OSC sequences, so the obvious version of this experiment silently measures nothing.
> The first run "showed" a frozen title and looked like a regression; it was the
> probe that was broken.

### 8.3 Upstream shipped a red test — how that was established

`1e3a898` adds/modifies
`ax_invalidation_after_quarantine_release_preserves_live_layout_state`, and it fails
at upstream's own tip. Its first four assertions pass (invalidation preserves the
window snapshot, workspace ownership, membership, and the layout node); the last one
fails — after rediscovery the window is no longer in `calculate_layout`, i.e. the
model survives an AX-element swap but the layout node does not. A window whose AX
element is replaced while it sits on a background workspace loses its tiling
position.

It is **not** our regression, and that was established rather than assumed by
running it on a detached `upstream/main` worktree. That check is now
`just upstream-test <TEST>` (§8.4) instead of a hand-built worktree, and the test is
in the `just test` skip list next to `topology_change_clears_stale_pending_hide_target`
with the reason recorded in the recipe comment.

**Panics in this test binary abort instead of unwinding, but the assertion message
is not actually lost** — `FORK.md` §8 said "no attributable failure", which is only
half true. `--nocapture` prints the panic message *before* the abort:

```bash
cargo test --lib -- <test> --test-threads=1 --nocapture
```

Without it you learn which test died; with it you learn why. `just upstream-test`
passes it for this reason.

### 8.4 §7's cadence rule, tested — and what it was missing

This is the first sync run under §7 ("sync on structural change, not on a
calendar"), and the numbers support it: **11 commits, 3 days, 2 additive hunks, zero
design decisions.** Compare the backlog: 38 commits, 16 hunks, five staged
checkpoints. Same project, same fork surface; the difference is elapsed time.

What §7 got wrong was not the rule but its *form* — it wrote the tripwires as shell
snippets in prose, which nobody runs. Three gaps closed, all in the `justfile`:

1. **`just sync` implemented the strategy §3 rejected.** It ran `git rebase
   upstream/main` plus a `git push origin upstream/main:main`. §3 chose merging
   precisely because rebasing replays our 47 commits against every upstream step.
   Anyone trusting the recipe over the prose would have done the wrong thing. It now
   merges, gates on build + tests, and refuses on a dirty tree. (Its *first*
   replacement merged commit-by-commit, which is its own trap — corrected the same
   day, see §9.4.)
2. **`just upstream-status` now runs the §7 tripwires** rather than printing a commit
   count — crate/workspace changes, added fields on `EventResponse`/`EventOutcome`/
   `Column`, `calculate_layout`/`update_layout` signature changes, and commit counts
   against our three churniest shared files.
3. **`just upstream-test TEST`** answers "our regression or theirs" in one command.

### 8.5 The allowlist hole, closed mechanically

`abdbcc8` ships four tests in a new `ui::menu_bar` module. They would have silently
never run — the **third** occurrence of this exact failure (`ui::stack_line` and
`ui::hints_bar` were the first two, and `FORK.md` §2.E claimed coverage that did not
exist for months). The response to the first two was a doc note asking the next
person to remember. That failed twice, so it is now a test:
`common::config::tests::just_test_allowlist_classifies_every_test_module`.

It parses the `test:` recipe out of the `justfile` via `include_str!`, walks `src/`
for every `#[cfg(test)] mod`, and asserts each one is either allowlisted or named in
an explicit `GUI_OR_DEFERRED` list. The invariant is *classification*, not
inclusion — 175 tests are excluded on purpose (they need a real GUI session, and
`bin::rift-cli` is not even reachable from `cargo test --lib`). A new module in
neither list fails loudly.

Proven by mutation, not just by passing: deleting `ui::menu_bar` from the recipe
makes it report `["ui::menu_bar::layout_library_tests"]`. It also immediately found
two modules nobody had classified (`bin::rift-cli::tests`,
`ui::mission_control::tests`).

---

## 9. Third sync — 2026-08-10 (same day), `7829780` → `3a52122` (3 commits)

```
behind before        3         ahead 49
conflict hunks       0         (all three auto-merged)
merge commits        d92cb5f, ed5a810, 112b563   (via `just sync`, then still
                                                  commit-by-commit — see §9.4)
curated tests        363 -> 370
```

| Commit | Subject | Verdict |
|---|---|---|
| `792370e` | fix: ghost windows appearing | TAKE — but it regresses floating state, see below |
| `7783d52` | fix: ghost windows after minimization | TAKE |
| `3a52122` | feat: !BREAKING! reveal layout state to ipc/cli (#439) | TAKE |

### 9.1 Why this happened the same day as §8

§8 merged `1e3a898` ("ax destroy not authoritative"), which fixes real sleep/wake
layout loss — upstream issue #440, confirmed fixed by the reporter. What the commit
message does not say, and the issue thread does, is that **the same change produced
ghost windows**, and the fixes landed hours after our merge point. Reading #440 to
the end was worth more than reading all eleven commit messages in §8.

`just upstream-status` — written that same afternoon — earned itself on first real
use: it flagged `crates/rift-protocol/src/commands.rs -50` as a structural tripwire,
which is `3a52122`'s breaking IPC change landing in the crate that holds our five
fork command variants and `MoveFocusArgs.activate`. All five survived; the merge
took zero hunks.

### 9.2 `792370e` trades ghosts for floating state — bisected

It breaks `wsid_rekey_preserves_floating_membership_and_position`: a window loses
`is_floating` and its stored position across a WindowServerId rekey, so a floated
window can snap back into the tiling after a rekey (sleep/wake, app relaunch).
Bisected across all four candidates on pristine worktrees:

```
7829780  PASS   (our §8 merge point)
792370e  FAIL   <- culprit
7783d52  FAIL
3a52122  FAIL
```

Kept anyway, deliberately: conflicts were zero, `3a52122`'s breaking IPC change was
free to absorb now and would not have been later (§7's whole thesis), the regression
is upstream's and will be fixed upstream, and its blast radius here is a recoverable
annoyance on a feature we have bound (`hyper + T`) but do not currently use. Skipped
in `just test` with the bisect recorded; drop the skip when upstream repairs it.

**Upstream main now carries three red tests** (§8.3 lists the other two). That is
worth knowing before you trust a green/red signal from their tree.

### 9.3 Verified live on the merged tip

22/23 automated checks on the running WM, in an isolated empty workspace using
throwaway windows so the live session was never touched — health, **zero ghost
windows across all 9 real windows**, every `query` subcommand round-tripping over
the new protocol, and the fork's own commands by observable effect
(`cycle-column-width` 1183→1437 px, `toggle-tabbed` collapsing two windows onto one
`(x=10, h=1064)` frame and restoring the 527/527 split, `move-node` reordering).

The 23rd was a **bad test, not a bad build**: `focus-column 0` appeared to no-op
because the check ran immediately after spawning a window, and the new window's own
async focus grab re-selected its column. Re-tested deterministically with every hop
targeting a non-selected column — 5/5 correct, so a no-op could not have passed.
Same failure mode as the `alacritty --title` trap in §8.2: **when a check fails
during a sync, confirm the probe before blaming the merge.**

### 9.4 The new `just sync` was wrong twice — found by testing it, not using it

Both §8's recipes shipped after exactly one happy-path run each. Pushed on whether
they were *actually* correct, two real bugs turned up — in the recipe written to
stop exactly this class of mistake.

**Bug 1: it merged commit-by-commit.** That reads like the careful option and is the
opposite. A range merge is the *smaller* conflict set, because git's 3-way merge
absorbs upstream's internal churn — including commits upstream later reverts.
Measured on `b31dddf..7829780` in a scratch worktree:

| strategy | conflicted files |
|---|---|
| range merge | **2**, both additive, both replayed by `rerere` |
| commit-by-commit | **7** at `1b69d8e` alone |

`1b69d8e` rewrites `Request::Raise` to carry `FocusConfirmation` precisely where our
fork carries `activate: bool`, and `7829780` reverts it. Stepping commit-by-commit
means resolving §2.B across 7 files to arrive exactly where a range merge lands for
free. §8.1 had *already written that down* as an argument against cherry-picking —
and the recipe did it anyway. Now: `sync` = `sync-to upstream/main`, with
`just sync-to <sha>` as the escape hatch for a backlog worth splitting (pick
boundaries that do not split a revert pair).

**Bug 2: bare `cargo` in the build gate.** It resolved fine in the blessed checkout
(direnv) and failed with `cargo: command not found` in any git worktree — which is
where a sync probe actually runs. This is the *identical* mechanism `FORK.md` §6
records for `just fmt`, reintroduced in a recipe written the same afternoon as that
correction was reaffirmed. Fixed: `nix develop {{justfile_directory()}} -c`, and the
duplicated second `cargo check` (run only to count errors) collapsed into one.

Also fixed: the recipe trusted `git merge`'s exit code, which is non-zero even when
`rerere` replays a resolution and leaves the file clean. It now asks git what is
unmerged and splits that list by whether conflict markers are actually present, so a
`rerere` replay reports "review before staging" instead of sending you to fix a file
with nothing left to fix.

**`rerere` earned its keep here, contradicting §4.6's 0% verdict** — it replayed both
`app.rs` and `config.rs` resolutions in the probe. §4.6's measurement stands for its
own case (a large delta where upstream had moved the surrounding code); the corollary
is narrower than it was written: rerere is useless across *big* gaps, valuable across
*small repeated* ones. Which is one more argument for §7's cadence.

Verified across all three paths before this was believed — reporting only, no claims
from a single run:

| case | setup | result |
|---|---|---|
| A | conflict `rerere` knows | reports "replayed … REVIEW before staging" |
| B | novel conflict (`0.999` vs upstream's `0.15`) | reports "needs manual resolution (8 files)" |
| C | clean range, bare worktree | merge → build → `just test` all run |

Plus the dirty-tree refusal, and `just upstream-test` on a *passing* test (exit 0,
temp worktree removed by its trap) — the path never exercised when it was written,
since both real uses hit failing tests.

---

## 10. Fourth sync — 2026-08-28, `3a52122` → `74ce00d` (22 commits)

18 days elapsed, 22 commits, **6 conflict hunks in 5 files**, 0 SKIPs. The largest
backlog since §4 and the cheapest per commit. Verdicts: **22 TAKE** (4 needing
rework), nothing refused.

Analysed in five parallel read-only clusters before touching the tree, because the
churn is concentrated in files we diverge in: `gesture_tap.rs`, `scrolling.rs`,
`reactor.rs`, `commands.rs`.

### 10.1 The headline: a clean auto-merge is not a safe auto-merge

Two fork features would have been silently lost through hunks git merged without
complaint. Neither is visible in the conflict list, and neither breaks the build.

1. **`9215d53 "improved gestures"` rewrites `gesture_tap.rs` — 870 of 878 lines** —
   onto a new `sys/gesture.rs` IOHID sensing layer, and drops our `scroll_speed`.
   The 2 conflict hunks *are* our three fork lines, so that part announced itself.
   The trap is the port: upstream moved `invert_horizontal` to **before**
   accumulation, so transcribing our old emission-site expression
   (`cfg.speed * if cfg.invert_horizontal { -accum } else { accum }`) would
   **double-invert**. Correct port is `cfg.speed * state.accum_dx`.
2. **The same commit's new `snap_to_nearest_column`** now *selects* a window (it
   used to only scroll), indexing the target column by the source column's row and
   never writing `Column.active`. That breaks §2.D tab memory two ways: wrong tab
   picked, and stale memory left behind. Textually clean, compiles, 547 tests still
   green. Only a new test catches it — added two, mutation-checked against
   upstream's exact block (both fail with `active: None`).

Lesson for §7: the tripwire counts *commits touching our churny files*, which
fired correctly here. What it cannot see is a semantic collision inside a file that
merges cleanly. Budget review time per **rewritten** file, not per conflict hunk.

### 10.2 Where upstream is simply better than us — and we deleted our version

`cce710a` + `0475baf` replace the admission model. Our fork carried
`ignore_app_rule: bool`, written at **five** reactor callsites. Upstream's
`manage_override: Option<bool>` is written **once** centrally
(`virtual_workspace.rs:1040`), is consulted by a single documented policy
(`WindowState::is_admitted`), is tri-state rather than boolean, and distinguishes
`AppRuleRejection::ExplicitRule` from `Heuristic` — a conflation our bool had.

The cluster analysis flagged our dropped lines as "a real semantic collision that
must be re-applied". Reading the replacement showed the opposite: re-applying would
fight the central write. **Dropped ours deliberately.** First time this sync series
has retired a fork feature as superseded rather than defended it.

Kept fork-local for the opposite reason: `active_workspace_columns`. `38eacff`
exposes ordered columns, but only privately in `query.rs` and only to enrich IPC
`WindowData` — not a superset. Same call as §5.2 made for `5c0b38d`: take the
upstream event, keep the in-process feed. `7d026c4`'s `layout_changed` likewise
carries no per-column pixel frames, so it cannot drive the hint bar.

### 10.3 Two tooling defects, both found by the tools themselves

**The `just sync` range merge was right and the per-commit alternative was a trap.**
`fad6498` does not compile alone: its own new test calls a helper that only
`5262221` adds, 25 seconds later. A commit-by-commit gate breaks there. §9.4's
measured preference for range merges is now confirmed by a second, independent
mechanism — not just a smaller conflict set, but *compilability*.

**The allowlist guard fired for the fifth time**, naming `actor::gesture_tap::tests`
(upstream's 5 new palm-rejection tests, all headless-clean). 547 → 549 tests.

**And a skip list rotted exactly like an allowlist.** The
`--skip ax_invalidation_after_quarantine_release_preserves_live_layout_state`
filter named a test `792370e` had already renamed — a **silent no-op**, which is
precisely the failure mode the comment above it warns about. Deleted; its successor
`current_ax_destruction_after_quarantine_release_removes_window` passes. The guard
covers test *modules*, not skip *names*, so this class is still unguarded. Verify
skips by deletion at every sync.

### 10.4 `#440` is fixed, the floating regression is not

`f2a9349 "fix: ghost windows + layout reset (#440)"` **reverts `792370e`'s reactor
branch** (deletes `handle_window_ax_invalidated` outright) but leaves the
`identify_stale_windows` predicate `792370e` changed — which §9.2 bisected as the
cause of losing floating state across a WindowServerId rekey. Different file,
different function: upstream repaired the ghost windows and never noticed the
rekey. `wsid_rekey_preserves_floating_membership_and_position` is still red on
pristine `upstream/main`, body unchanged in this range.

**Skip stays, re-attributed** from "792370e" to the surviving predicate. Two skips
now, both upstream's.

### 10.5 Known, unfixed: centered floats and our external-bar insets

`54ac143` adds `toggle_window_floating = { center = true, size = ... }`, closing the
last gap in `FORK.md` §8 roadmap item 3. Its screen rect is built from
`active_size` (the **tiling** area, which *our* `external_bar_top/bottom` insets
shrink) combined with `visible_space_centers` (the **native** screen centre,
`reactor.rs:3323`). Mixing them offsets a centered float by half the total inset.

Fork-induced: upstream has no insets, so for them the two agree. **Latent** —
nothing binds the options form (`rift.default.toml:550` and `dev-config.toml:149`
both bind plain `toggle_window_floating`; the option forms are commented examples).
Not fixed here on purpose: the honest fix extracts the inset math at
`managers.rs:441-475` into one helper shared with `visible_spaces_for_layout`, and
that refactor does not belong in the same commit as a 22-commit merge. Do it before
binding `center = true`.

### 10.6 Verified

549 tests, `fmt` clean, 0 behind upstream, `.app` built and installed.

Live, 16/16, on scratch windows in an empty workspace (never the user's): all six
`query` subcommands round-trip after `a4d70a4` moved the Mach server onto the
reactor; `focus-column`, `cycle-column-width` (1180 → 1435), `consume-or-expel`,
`toggle-tabbed` (2 windows sharing one frame) all work over the rewritten protocol;
`scroll-strip`/`snap-strip` — the commands gestures emit — drive the new handler
with zero panics; no hints-bar warnings; workspaces restored exactly.

**A live probe was the wrong instrument for tab memory, and said so twice.**
`container_tree` reports a scrolling column's `layout_kind` as `Vertical` whether
or not it is tabbed, so tabbedness is not observable over IPC; and
`consume-or-expel` can leave a single column where `move-focus left/right` is a
no-op that passes vacuously. Both produced a confident wrong reading before the
in-process test settled it. Fourth bad probe in this series (`--title`,
`focus-column` spawn race, the pid-under-`id` ghost check, now this): **when a live
check disagrees with the code, suspect the probe first.**

### 10.7 What is now enforced instead of remembered

This sync's failures were all the same shape: **a curated list or a habit that
nothing checked.** A dead `--skip`, a test module upstream added, a fork feature
dropped by a clean auto-merge, a push state read from a stale ref. Prose asking the
next person to remember has now failed on every one of these at least once.

So the load-bearing invariants moved out of prose, split by *when* they can be
caught. Both halves are listed in `FORK.md` §6 "Enforced guards".

**TTSR rules (`.omp/rules/*.md`) — fire while the mistake is being made.** Five,
covering: pushing to `upstream`, rebasing onto `upstream`, reading `origin/<b>` from
a stale ref, adding an unclassified `#[cfg(test)]` module, and writing a
`state.selected` assignment without pairing `Column.active`. The first two interrupt;
the rest fold a reminder into the tool result.

**Guard tests — fire on every `just test`.** `just_test_skips_all_name_a_real_test`
is new here, closing the hole §10.3 found. The allowlist guard has now caught five
modules. The `snap_*` pair guards §10.1's silent collision.

Three things learned building them, all of which cost a correction:

1. **Write the probes, then trust nothing else.** Every rule carries
   `probes: fire/silent`, and `omp ttsr test --rule <f>` runs them through omp's
   real matcher. **Two of five regexes were wrong on first write** — `git\s+rebase`
   missed `git -C <dir> rebase upstream/main`, and an unbounded gap let
   `git push origin x && git fetch upstream` match as a violation. Both were found
   by running the probes, not by reading the regex.
2. **A false positive is worse than it looks.** `repeatMode` defaults to `once`, so
   one bad fire *burns the rule* for the session — the next, real violation passes
   silently. Hence gaps that exclude `;`, `&`, `|` rather than merely shorter gaps.
3. **Rules are not a place to move tests to.** A rule matches a *stream*, so it can
   only ever see what is being written, never the resulting state. The §2.D pairing
   rule prompts you to decide; the `snap_*` tests prove the decision held. Deleting
   either in favour of the other loses the property that made it work.

One structural caveat for whoever reads this next: **rules load at session start**,
so a rule authored mid-session does not protect that session. The ones above were
validated with `omp ttsr test` rather than by triggering them live, which is the
honest bound on that verification.

---

## 11. Fifth sync — 2026-09-18, `74ce00d` → `83f3b66` (77 commits)

Planned and executed the same day. **§11.10 is the outcome**; §11.0–§11.9 are the
plan as written *before* the merges, kept unedited so the predictions can be
scored. Everything below is measured on the real trees — no estimates.
Line numbers and hunk counts come from a throwaway full-jump
probe, recreated with:

```bash
git worktree add --detach /tmp/rift-probe HEAD
git -C /tmp/rift-probe -c rerere.enabled=false merge --no-commit --no-ff upstream/main
git -C /tmp/rift-probe diff --name-only --diff-filter=U    # 12 UU
git -C /tmp/rift-probe status --short | grep -E '^(UD|DU)'  # the 2 deletions
```

Read §11.3 and §11.4 before touching a merge; §11.6 is the order to work in.

### 11.0 Situation (measured)

```
merge-base            74ce00dd8cb6cf15cba3ef7459af70ddff51a562   (§10's tip)
behind / ahead        77 / 64
upstream churn        85 files, +9480 / -4956
full-jump conflicts   14 files — 12 UU (13 hunks), 2 UD
new upstream files    src/actor/input.rs (+1850)
                      src/layout_engine/systems/floating.rs (+807)
                      src/sys/cocoa.rs (+329, entirely cfg-gated)
                      crates/rift-protocol/src/transport.rs (+65)
                      .github/workflows/{tag,docs}.yml
upstream test delta   reactor/tests.rs 141 -> 165 #[test]
```

The two **`UD`** entries are the headline: `src/actor/event_tap.rs` and
`src/actor/gesture_tap.rs` are *modified by us, deleted by them* — upstream folded
both into `src/actor/input.rs` (`2ec9881`, extended by `1c57b5f`). Fork commits
`4b460a0` (configurable horizontal scroll speed) and `42c9376` (hint-bar click
routing) live in those files.

### 11.1 Tripwire verdict — this is a sync-now, and it is late

Every §7 tripwire fired:

| Tripwire | Hit |
|---|---|
| `Cargo.toml` structural change | `[features] default/experimental/custom-event-loop/disable-axuielement-destroyed` (`1c57b5f`, `83f3b66`); `continue` + `futures-task` deps dropped (`0d17a7a`); version `0.1.0` → `0.5.9` |
| field added to `Column` | `node_id` (`7e76bdb`) — lands beside our `tabbed`/`active` |
| `EventOutcome` fan-out rewritten | `52a5425` (`outcome.rs` +63/-161), `f9854b7` (`WindowLayoutInfo` replaces the big tuple), `c33cf4a` |
| `scrolling.rs` rewritten | `cf102d4` (#320) + `b3e916e` + the two #466 commits |
| crate moved / added | `crates/rift-protocol/src/transport.rs`; `ContainerTreeNode` gains two required fields |

§7's rule was "sync on structural change, not on a calendar". Five structural
changes accumulated over three weeks. The cost is visible in §11.3: one 1850-line
file replacing two files we had edited.

### 11.2 Verdicts — all 77 commits

Chronological index `#N` is `git log --reverse 74ce00d..upstream/main`. Verdict
vocabulary as in §3: `TAKE`, `TAKE-REWORK`, `DEFEND-OURS`, `DROP-OURS`, `SKIP`.

**Totals: 59 TAKE · 16 TAKE-REWORK · 2 SKIP · 0 DROP-OURS · 0 ALREADY-HAVE.**
Nothing upstream shipped here duplicates a fork fix, and nothing of ours is
superseded outright — the whole cost is in 16 reworks and the twelve hazards in
§11.4. Per cluster: A 21 · B 17 · C 16 · D 12 · E 11 = 77.

#### A. AX / app-lifecycle / window-discovery (21)

| # | sha | subject | verdict | note |
|---|---|---|---|---|
| 2 | `46f5ba0` | reject ordered-out windows during discovery (#460) | TAKE | fork owns nothing in that region |
| 3 | `59b9033` | layout/workspace resets (#456)(#440) | TAKE | reconciles from the WindowServer-authoritative snapshot |
| 4 | `e4aa60d` | axwindows update resetting layouts (#440) | TAKE-REWORK | `WindowInventoryToken` pipeline replaces `GetVisibleWindows`; ~5 fork call sites break **loudly** (`main_window.rs:351`, `testing.rs:141,554`) |
| 5 | `f9b152d` | duplication of child windows (#462) | TAKE | `seen_wids` dedup |
| 6 | `6d7b21c` | refresh stale ax identifies | TAKE-REWORK | adds `RaiseTargetsMissing` self-heal **inside** our rewritten `handle_raises` — app.rs conflict |
| 9 | `b1060de` | window_spaces returning >1 spaces | TAKE | `spaces.rs` only |
| 10 | `21efa52` | reject snapshots with `space: None` | TAKE | one-line tighten + tests |
| 14 | `372fcd2` | reap child when NOTE_EXIT beats zombie (#465) | TAKE | `sys/dispatch.rs`; no overlap with `c32893b`/`b31dddf` |
| 19 | `d095bb6` | refresh stale AX inventories after display wake | TAKE | needs #4's `in_flight` map |
| 20 | `d8214ba` | don't reset layout across display sizes (#440) | TAKE | size-keyed restore |
| 21 | `e7eb854` | loginwindow activation authoritative (#456) | TAKE | `process.rs`/`spaces.rs`; **complements** §2.B, never touches it |
| 22 | `251fe8b` | don't destroy on InvalidUIElement (#456) | TAKE | `WindowInvalidated(source)` keeps the logical window |
| 24 | `1714f90` | windowtitlechanged not firing (#471) | TAKE | app-level observer; makes our §2.C/§2.E titles *fresher* |
| 25 | `beeac0e` | track dragged frame while swap pending (#475) | TAKE | `DragState::PendingSwap`; hint-bar clicks never enter `DragState` |
| 28 | `556b7ad` | only rerun title rule | TAKE | `engine.rs:2837`, far from our hunks |
| 49 | `0121cf2` | keep self-initiated focus raises quiet (#485) | TAKE-REWORK | `Quiet::No` → `Quiet::Yes` at two sites that carry our `activate` — see hazard H1/H2 |
| 53 | `3689e81` | prevent duplicate app threads | TAKE | `ExitGuard`→`AppExited`; merges around our `wm_controller` wiring |
| 73 | `311d57e` | handle slow launching apps (#491) | TAKE | **must land with #75** |
| 75 | `c4ad9b5` | apps reporting wrong pid at startup (#491) | TAKE | partially reverts #73; threads the Carbon pid |
| 76 | `5ab67c2` | add another window close event (#486) | TAKE | SLS-authoritative `WindowClosed(wsid)`; the guard that makes #77 safe |
| 77 | `83f3b66` | experimentally ignore AXUIElementDestroyed | TAKE | inert: `disable-axuielement-destroyed` is non-default, and `nix/package.nix` passes no `--features` (verified: `cargoExtraArgs = "--locked --package rift-wm --bins"`). **Do not enable.** |

Closest call to an `ALREADY-HAVE`: our `7ceb4d5` `warn!` *diagnoses* exactly what
`6d7b21c` *cures* (raise to a stale AX element → silent 250 ms `raise_manager`
timeout). Keep both — upstream's fix covers `NotFound`/`InvalidUIElement`, our warn
still covers every other `RaiseError`.

#### B. Layout engine + layout systems (17)

| # | sha | subject | verdict | note |
|---|---|---|---|---|
| 15 | `ed3b6c5` | master-stack: preserve membership on removal (#469) | TAKE | `master_stack.rs` is fork-free |
| 26 | `b9e6d6c` | bsp: raise only the focus target (#478) | TAKE | `bsp.rs` is fork-free |
| 27 | `6baa565` | avoid redundant layout passes | TAKE | reverted by #40; **never cherry-pick alone** |
| 30 | `9b4f133` | traditional: panic when join collapses parent (#470) | TAKE | our `46a983a` proptest (`traditional_tiles_partition_the_area`) does **not** cover `join` — not an `ALREADY-HAVE` |
| 32 | `3cd61ac` | add tests | TAKE | free |
| 33 | `563d54a` | persistence matching more permissive (#481) | TAKE | `persistence/` is fork-free; our "layout snapshotting" (`d5e827e`) is a `justfile` recipe, not engine code |
| 35 | `f9854b7` | `WindowLayoutInfo` replaces large tuple | TAKE-REWORK | compile-enumerated sweep |
| 36 | `52a5425` | clean up event outcome | TAKE-REWORK | `outcome.rs` −161; `outcome.rs` is fork-free, but our `EventResponse.activate` must survive the struct edit |
| 37 | `b3e916e` | cleanup layout system | TAKE-REWORK | touches `scrolling.rs`/`systems.rs` |
| 40 | `7a59369` | Revert #27 | TAKE | net-zero with #27 (reverse patch-id differs: an intervening commit widened the block) |
| 41 | `c33cf4a` | add missing outcome | TAKE | +5 lines in `outcome.rs` |
| 50 | `cf102d4` | hide off-screen scrolling columns + boundary (#320) | TAKE-REWORK | see §11.5 — this is **our dropped "off-left snap", re-invented better**, plus a `move_focus` early return |
| 51 | `f5c2b76` | build error | TAKE | fixes #50 |
| 52 | `1628c1c` | floating layout | TAKE-REWORK | new `LayoutMode::Floating` + `floating.rs`; +285 in `engine.rs`, +57 in `systems.rs`, +45 in `traditional.rs` |
| 67 | `8b07cef` | cleanup | TAKE | |
| 68 | `e4cf6f0` | cleanup | TAKE | |
| 69 | `46e1ff5` | layout cleanup | TAKE | |

#### C. Query/perf gating, IPC, protocol, event surface (16)

| # | sha | subject | verdict | note |
|---|---|---|---|---|
| 7 | `9deb843` | dock orientation values | TAKE | `sys/screen.rs` fork-free |
| 11 | `8a1cad0` | warp cursor after `move_node` | TAKE | gated on `mouse_follows_focus`; adds `EventOutcome.post_arrange_mouse_warp` |
| 12 | `afa767f` | real window focus on `focus_display` (#332) | TAKE-REWORK | new `focus_window_raise_request()` builds a **4-field** `RaiseRequest`; ours has 5 |
| 13 | `b67cf2e` | warp cursor to focused window | TAKE | `focus_window_center` on `DisplayFocusPayload` |
| 16 | `487168d` | frame in `container_tree` (#466) | TAKE-REWORK | `ContainerTreeNode.frame` now required |
| 17 | `7e76bdb` | stable `node_id` (#466) | TAKE-REWORK | `Column.node_id` — ~15 fork-only `Column` literals break |
| 18 | `7821dd3` | selection changed event (#466) | TAKE | additive `EventKind::SelectionChanged`; `RIFT_EVENT_TYPE` now `event.kind()` |
| 38 | `be3bbea` | memory leaks in ipc | TAKE | mach dead-name notifications; `ipc.rs` is fork-free |
| 45 | `6d66b90` | gate `window_spaces` query | TAKE | different pipeline stage from our bar dedup |
| 46 | `a8a1079` | only request stale observations when needed | TAKE | splits `identify_stale_windows` |
| 47 | `d7c7734` | reuse native id resolutions | TAKE | `NativeWindowIdentity` memo |
| 48 | `d3be1bc` | bulk window attributes (2–4×) | TAKE | `AXUIElementCopyMultipleAttributeValues` |
| 63 | `52b2846` | move-workspace-to-display (#314) | TAKE-REWORK | big `engine.rs` refactor; **not** roadmap item 4 |
| 66 | `634be2b` | query windows/workspace by display id | TAKE | additive |
| 72 | `44d7fa1` | more display/space scoping | TAKE | restructures `QueryCommands`/`DisplayCommands` only; our verbs live in `LayoutCommands`/`ExecuteCommands` |
| 71 | `05fbe10` | release display-churn gate on an edge (#503) | TAKE | kills a 320-refresh/241-raise storm; doubly valuable for us because spurious raises also trigger §2.B activation |

#### D. Input / event pipeline (12)

| # | sha | subject | verdict | note |
|---|---|---|---|---|
| 8 | `0f70787` | hotkey parsing/key list | TAKE | fixes **9 letters** (I J K L M N O P U) rendering as `Other`, which silently dropped bindings; #54 imports its helpers |
| 54 | `2ec9881` | combine event taps | TAKE-REWORK | **the UD migration** — §11.3 item 1 |
| 55 | `44af72d` | brandng | TAKE | README |
| 56 | `1c57b5f` | experimental custom event loop | TAKE | fully `#[cfg(feature)]`-gated, `default = []`; take the code, never enable it |
| 57 | `0d17a7a` | get rid of continuation | TAKE | drops two deps |
| 58 | `f3264ba` | thread mouse location through | TAKE | `input.rs` internal |
| 59 | `c1f809a` | nits on custom event loop | TAKE | `sys/cocoa.rs`, gated |
| 60 | `c975499` | branding | TAKE | README/roadmap |
| 61 | `6b1db00` | focus-follows-mouse flakiness | TAKE-REWORK | pull-based `MouseFocusPublisher`; its `get_window_at_point` now skips **all** rift-owned windows — strictly good for §2.E |
| 62 | `28f0733` | get rid of gesture cooldown | TAKE | deletes `FocusSuppression`; **do not defend it** — obsolete under pull-based FFM |
| 70 | `b48fde1` | gesture scrolling | TAKE | contact-only decode; emission site untouched, so our speed knob is unaffected |
| 74 | `e9eafba` | pause mouse focus while menu open | TAKE | `NSMenuDelegate` → `MenuOpened`/`MenuClosed` |

#### E. UI, CI, docs, chore (11)

| # | sha | subject | verdict | note |
|---|---|---|---|---|
| 1 | `73a475f` | perf: menu bar | TAKE | `sig()` shrink |
| 23 | `dd9ad8f` | perf: menu bar | TAKE | `MenuIconRenderKey` render-skip cache; **no** shared surface with our hint bar — §5.2 bullet 2 holds verbatim |
| 29 | `a3e6bfb` | trigger docs build on release | **SKIP** | `repository-dispatch` → `acsandmann/rift-docs` with `HOMEBREW_TAP_TOKEN`; the missing secret fails `build-and-publish`, which skips our `update-nix` through `needs:` |
| 31 | `9d6913c` | update latest docs on every commit | **SKIP** | new `docs.yml`, same hardcoded upstream repo; a permanent red run on the fork |
| 34 | `83830a7` | fmt | TAKE | `rustfmt.toml` is byte-identical HEAD↔upstream |
| 39 | `48e2f1e` | THE DOCS | TAKE | re-points README/menu links at upstream's docs site; adds no in-repo infra |
| 42 | `4bb2328` | cli docs | TAKE | clap fixes; our four verbs untouched |
| 43 | `04defb3` | cli docs^2 | TAKE | doc comments |
| 44 | `f2691fd` | fmt | TAKE | |
| 64 | `072411b` | better release workflow | TAKE-REWORK | tag-verify, `--package rift-wm`, packaged-version check, `build.rs` `RIFT_VERSION` stamping, **new `tag.yml`** |
| 65 | `1444fa0` | bump 0.5.9 | TAKE-REWORK | clean take-theirs in `Cargo.toml`; `nix/package.nix:116` must be hand-bumped |

### 11.3 The six pieces of real work

Ordered by cost, not by date.

**1. The input fold — `event_tap.rs` + `gesture_tap.rs` → `input.rs` (`2ec9881`).**
Upstream's rewrite is near-verbatim, so every fork hunk has an exact destination:

| Fork hunk (deleted file) | Destination in `src/actor/input.rs` |
|---|---|
| `42c9376`: `hints_bar` import, `EventTap.hints_bar_tx` / `hints_bar_hit_rects` + `new()` params | `struct Input` (~57), `Input::new` (~258) |
| `42c9376`: LeftMouseDown hit-test → `hints_bar::Event::MouseDown(loc)` | `on_event`'s `LeftMouseDown \| RightMouseDown` arm, after the `state.stack_line_enabled` block (~592) |
| `4b460a0`: `ScrollConfig.speed = g.scroll_speed.clamp(0.1, 20.0)` | `struct ScrollConfig` (~1327) + `from_config` (~1336) |
| `4b460a0`: `let delta = cfg.speed * state.accum_dx;` | `handle_scroll` (~1679); upstream already applies `invert_horizontal` before accumulation, same order our comment documents — keep the comment |
| wiring | `src/bin/rift.rs` `spawn_supervised("input", …)` (~387): delete the `EventTap::new`/`GestureTap::new` blocks, extend `Input::new` with `hints_bar_tx.clone()` + `hints_bar_hit_rects` |
| module list | `src/actor.rs`: drop `event_tap`/`gesture_tap`, keep `hints_bar`, take `input` |

**The one piece that is *not* a port: the dynamic event mask.** Our old
`build_event_mask(keyboard, mouse_move)` added Left/RightMouseDown/Up
*unconditionally*. Upstream's 6-arg `build_event_mask` + `desired_event_mask`
(~170) enable mouse buttons only when
`state.event_processing_enabled && (state.stack_line_enabled || state.mouse_hides_on_focus)`.
So a faithful port of our hit-test block **still dies** whenever `stack_line` is
disabled — the configuration §2.E actually recommends. Required: add
`hints_bar_enabled` to `struct State`, initialize from
`config.settings.ui.hints_bar.enabled` beside `stack_line_enabled` (~276), refresh
it in the `Request::ConfigUpdated` arm (~429-456 — which also gives our scroll-speed
knob hot reload for free through `build_gesture_handlers`), and OR it into the
buttons argument.

**2. `activate` through upstream's new raise helpers.** Upstream extracted
`focus_window_raise_request()` (`command.rs`, `afa767f`) and flipped
`focus_quiet` to `Quiet::Yes` at two sites (`0121cf2`). Ours is the 5-tuple
`RaiseRequest(wids, token, sequence_id, quiet, activate)` and `EventResponse`
carries `activate: bool` (upstream's has only `changed/raise_windows/focus_window/boundary_hit`).
Resolutions, per site:

- `events/window.rs` (~477, competing edit): `focus_quiet: Quiet::Yes` **and**
  `activate: false`. Neither side alone is correct.
- `events/command.rs` (~405): take upstream's helper, add `activate: false` inside it.
- `app.rs` `handle_raises` (~691-727): keep our destructure, our pre-captured
  `(first, count)` and our `7ceb4d5` `warn!`; change `handle_raise_request` to
  upstream's `wids: &[WindowId]` **plus** our trailing `activate: bool`; add
  upstream's `RaiseTargetsMissing` emission for
  `AxError::NotFound | AXError::InvalidUIElement`.
- 35 `activate:` literals exist across 7 files; the compiler enumerates every one.

**3. `e4aa60d`'s window-inventory token migration.** `Request::GetVisibleWindows`
and the 4-field `Event::WindowsDiscovered` are gone, replaced by
`WindowInventoryToken { request_id, topology_revision }` +
`Request::RefreshWindowInventory`. Known fork-side sites: `main_window.rs:351`
(a §2.B test), `testing.rs:141` (`discover_test_windows`), `testing.rs:554`. Wide
but mechanical, and every missed site is a hard error.

**4. `Column.node_id` / `ContainerTreeNode` sweep, and two additive unions.**
`7e76bdb` adds `Column.node_id` and `487168d` adds `ContainerTreeNode.frame`;
both are required fields. Upstream's own constructor sites auto-merge with our
`tabbed`/`active`, but ~15 **fork-only** `Column` literals (tests, the
`toggle_selection_tabbed` split, expel paths) break. Resolve with `node_id: 0` —
safe **only because** `Column::stable_node_id()` falls back to
`column_node_id(first_window)`; do not "simplify" that fallback away, or
fork-created tabbed columns emit colliding ids (upstream's `assert_stable_unique_ids`
in `systems.rs` catches duplicates — keep it).

The two conflicts here are the familiar additive pattern (§4 playbook) and both
resolve as *both sides*:

- `scrolling.rs` L57-77: our `CachedTabGroup` struct vs upstream's
  `packed_window_id`/`window_node_id`/`column_node_id` helpers.
- `systems.rs`: our `toggle_selection_tabbed` / `cycle_column_width_preset` trait
  methods vs upstream giving `windows_in_selection_stack` /
  `parent_of_selection_is_stacked` / `unjoin_selection` **default bodies** (needed
  by `floating.rs`). Keep our two methods, take their defaults.
- `engine.rs` L211-273: our `active_workspace_columns` (the hint-bar feed) vs
  upstream's new `pub(crate) calculate_workspace_layout`. Keep both.

**5. Release/CI and the version jump.** `release.yml` is fork-owned only by
*addition* — `9575958` appended an `update-nix` job to upstream's
`build-and-publish`, so `072411b`'s hardening merges clean and there is exactly one
conflict hunk (ours: the whole `update-nix` job; theirs: `a3e6bfb`'s docs-dispatch
step). Resolve `@ours`. Then, in the same merge commit:

- **`tag.yml`** (new, merges clean) auto-tags `v<version>` on any push to `main`
  that touches `Cargo.toml`, using the default `GITHUB_TOKEN`. On this fork that
  mints a fork-local `v0.5.9` pointing at a different commit than upstream's —
  a permanent collision for any dual-remote clone. Guard with
  `if: github.repository == 'acsandmann/rift'` or delete.
- **`docs.yml`** (new, merges clean) dispatches to `acsandmann/rift-docs` on every
  `main` push using a secret we do not have: guard or delete (verdict `SKIP`, #31).
- **Version skew.** `Cargo.toml` takes `0.5.9` with no conflict; `nix/package.nix:116`
  still says `0.1.0` and is only auto-updated by `update-nix` on a *published*
  release. Hand-bump it.
- **`build.rs` `RIFT_VERSION`.** All its `git()` calls fail in the crane sandbox, so
  nix-built binaries will report `0.5.9+unknown.dirty`. Either plumb an env override
  from `nix/package.nix` or accept the cosmetic suffix — decide, don't discover it.

**6. The layout refactor group + floating layout.** `f9854b7`/`52a5425`/`b3e916e`
plus the four `chore: cleanup` commits are compile-enumerated churn over
`engine.rs` (+944/-531) and `outcome.rs` (+63/-161); `outcome.rs`, `bsp.rs`,
`master_stack.rs` and `persistence/` carry **zero** fork commits, so the only fork
surface is `EventResponse.activate` and our `engine.rs` additions.
`1628c1c` adds a genuine new layout mode (`LayoutMode::Floating`, `floating.rs`,
config + menu-bar entries) — adopt as-is, and note the §11.5 inset consequence.

### 11.4 Hazard register — clean merges that silently drop fork behaviour

This is the §10.1 failure mode. Each row merges *without a conflict* (or conflicts
in a way that a plausible "take one side" resolution satisfies) and loses a fork
behaviour with no test failure.

| # | Where | What is lost if resolved naively | Detection |
|---|---|---|---|
| H1 | `events/window.rs` ~477 | `@ours` keeps `Quiet::No` → silently reverts #485 (a stale main window switches workspace under FFM) | upstream's new test `focus_follows_mouse_raise_is_quiet_…` — **only if the `tests.rs` hunk is taken** |
| H2 | `events/command.rs` ~405 | `@ours` drops #485's quiet for the `focus-window` IPC command | **no test covers the command-site quiet** — add one asserting `focus_quiet == Quiet::Yes` |
| H3 | `app.rs` `handle_raises` | `@ours` loses `RaiseTargetsMissing` entirely: post-wake raises to stale AX elements go back to silent 250 ms timeouts | `grep RaiseTargetsMissing src/actor/app.rs` must hit after the merge; no test covers the emission |
| H4 | `reactor.rs` ~443 | upstream deletes the wiring block that holds **our `hints_bar_tx` registration** alongside its own `gesture_tap_tx` removal → §2.E hint bar disconnected, still compiles | `just test` hint-bar filters, plus the bar simply never renders |
| H5 | `src/bin/rift.rs` `spawn_supervised("input", …)` | taking upstream's `Input::new` call verbatim leaves `hints_bar_hit_rects` unused → every hint-bar click dead | `cargo check` unused-variable **warning** only |
| H6 | `input.rs` `desired_event_mask` | faithful port of our hit-test block still yields no `LeftMouseDown` when `stack_line` is off (§11.3 item 1) | no test; live: `stack_line.enabled=false` + `hints_bar.enabled=true`, click a chip |
| H7 | `.github/workflows/{tag,docs}.yml` | arrive as new files with zero conflicts; fork starts tagging and dispatching to upstream's docs repo | post-merge sweep of `.github/workflows/` for `repository-dispatch` / `acsandmann/rift-docs` |
| H8 | `release.yml` hunk #3 | `@theirs` deletes the whole `update-nix` job | `grep update-nix .github/workflows/release.yml` |
| H9 | `reactor/testing.rs` (`05fbe10`) | `forwarded_space_state` default `releases_display_churn_refresh_quarantine` flips `false → true`; fork tests built on that helper keep passing while covering a different regime | re-read the churn-adjacent fork tests after the stage that lands `05fbe10` |
| H10 | `Cargo.toml` dev-deps | upstream **deleted `proptest = "1"`**; our `46a983a` proptests need it. The probe merge kept our line by line position, i.e. by luck | `grep -n proptest Cargo.toml` after every stage |
| H11 | `reactor.rs` log arms (`b31dddf`) | new `WindowInvalidated`/`WindowClosed`/`WindowsDiscovered{..}` variants land next to our trace-demotion comment; a re-resolution could promote `WindowTitleChanged` back to `debug!` (the 1108 → 5 MiB/day fix) | confirm `WindowTitleChanged` stays in the `trace!` arm |
| H12 | `Event::ApplicationThreadTerminated` (`3689e81`) | upstream removed both `send_event` sites in favour of `ExitGuard`→`AppExited`; the variant still exists, so any fork test awaiting it compiles and waits forever | `grep -n ApplicationThreadTerminated src/actor/reactor/tests.rs src/actor/reactor/main_window.rs` |

### 11.5 Where we stand — what to retire, rewrite, and newly own

**§5.2 bullet 1 is factually obsolete; its conclusion survives on new grounds.**
That bullet rejected upstream's broadcast as a hint-bar feed because it was
"geometry-free" and "fires on a strict subset". Both claims are now dead:
`487168d` puts per-node **pixel frames** in `container_tree` (root = tiling area,
columns = union of window target frames) and `7e76bdb` adds **stable `node_id`s`**,
and `LayoutChanged` fires on every `layout_changed` outcome. Keep our
per-layout-apply feed anyway, for four *different* reasons, all verified against
`upstream/main:crates/rift-protocol/src/events.rs`:

1. no `WindowMember.label`/title in `LayoutStateData` — our `show_titles` and hover
   pill would need a `GetWindows` join per event;
2. no cross-workspace occupancy — the workspace rail needs every workspace's pids,
   the event carries only the active workspace;
3. no representation of our fork-only `Column.tabbed` (upstream's tree says
   `role:"column", layout_kind:Vertical` either way — the same blindness `e533c69`
   already recorded);
4. mach + per-subscriber `serde_json` delivery would discard the `ee2abe4` dedup and
   coalescing layers.

**Rewrite that bullet when the sync lands.** One genuine simplification is now
available: source the feed's frames from upstream's new public
`LayoutEngine::calculate_workspace_layout` instead of any parallel derivation.

**Our dropped "off-left snap" is now upstream's own design (`cf102d4`).** §2's
"Dropped / superseded" list says we reverted a per-column `x` clamp because
"upstream already clamps the scroll offset". Upstream has now shipped the per-column
clamp itself — and better: it parks off-viewport columns at `screen.origin.x -
column_width` / `screen.max().x`, i.e. beyond the *physical* display rather than the
tiling edge, so outer-gap pixels cannot leak and adjacent displays cannot overlap.
It also adds an early `return EventResponse::default()` that stops horizontal
`move_focus` from crossing displays in the scrolling layout. Amend the dropped-list
entry: the revert was still right (we would have conflicted every sync for a worse
version of this), and the feature arrives for free.

**Our hint bar is immune to that parking — by design, already.**
`active_workspace_columns`'s doc comment states it exists precisely because
"the scrolling layout parks columns that are scrolled off the right edge at a shared
sliver x, so grouping windows by frame position merges distinct columns". `7438671`
made the feed read `container_tree` structure instead of frames; `cf102d4` extends
the parking to both edges and off-display, and the feed does not care.

**`1628c1c` makes §10.5 actionable, not moot.** `floating.rs` computes
`default_frame` and `cascade_frame` from the raw `screen` rect
(`screen.mid().x - size.width/2.`), not from the inset-adjusted tiling area, so the
new floating layout centres windows *under* an external bar exactly like §10.5's
centered floats. Our `6f9ebca` insets are the only thing that knows about the bar.
Fork follow-up after the sync: pass the inset-adjusted rect into
`FloatingLayoutSystem`'s frame helpers — a real feature, scoped to one file, and now
with upstream test coverage to extend (`floating.rs:753+`).

**`FORK.md` §8 roadmap after this sync.**

| Item | Status |
|---|---|
| 1. Tabbed column display | ours; upstream still has nothing |
| 2. Per-column width presets | ours; upstream still has nothing |
| 3. Richer window-rule open actions | unchanged since §5.3 (`open_maximized` still absent) |
| 4. Workspace reordering / named workspaces | **still open.** `52b2846` moves the active workspace to another *display* at the same ordinal — adjacent ergonomics, not reordering |
| 5. Interactive mouse resize | not delivered |
| (new) floating-layout insets | **new fork item**, created by `1628c1c` (above) |

### 11.6 Stage plan — ten checkpoints

`git merge <sha>` in order, each gated on
`cargo check --workspace --all-targets` + `just test` **before** its merge commit is
finalized (§7). Conflict counts are measured from the full-jump probe and are
upper bounds per stage.

| Stage | Checkpoint | `#` | Commits | Conflicts | The work |
|---|---|---|---|---|---|
| 1 | `21efa52` | 1–10 | 10 | app.rs ×1 | §11.3 item 3 (inventory tokens) **and** item 2's `handle_raises` merge. Gate hard — this is the heaviest stage after 8. |
| 2 | `372fcd2` | 11–14 | 4 | command.rs ×1 | cursor-warp trio; `RaiseRequest.activate` into `focus_window_raise_request` |
| 3 | `251fe8b` | 15–22 | 8 | scrolling.rs ×1, engine.rs ×1 | #466 surface: `Column.node_id` + `ContainerTreeNode.frame` sweep; wake/session family |
| 4 | `83830a7` | 23–34 | 12 | release.yml ×1 | mostly free; **`docs.yml` arrives here — guard it in this merge commit** (H7) |
| 5 | `f2691fd` | 35–44 | 10 | engine.rs, systems.rs | the refactor group (`WindowLayoutInfo`, outcome cleanup); compile-enumerated |
| 6 | `0121cf2` | 45–49 | 5 | window.rs ×1, command.rs ×1 | perf quartet (free) + the `Quiet::Yes` flip → H1/H2 |
| 7 | `1628c1c` | 50–52 | 3 | scrolling.rs, systems.rs | off-screen parking + boundary early-return + floating layout |
| 8 | `28f0733` | 53–62 | 10 | actor.rs, bin/rift.rs ×2, reactor.rs, mission_control.rs ×2, event_tap.rs **UD**, gesture_tap.rs **UD** | **the input fold** (§11.3 item 1) → H4/H5/H6. Do **not** split this range: `f3264ba`/`28f0733`/`b48fde1` rewrite the exact regions our hunks target, and a mid-range boundary migrates them onto code the next commit deletes. |
| 9 | `46e1ff5` | 63–69 | 7 | release.yml ×1 | release hardening + `0.5.9` + `tag.yml` guard + `nix/package.nix` bump (§11.3 item 5). `072411b` and `1444fa0` **must land together**. |
| 10 | `83f3b66` | 70–77 | 8 | — | gesture perf, churn-gate edge (H9), display scoping, slow-launch pair (`311d57e`+`c4ad9b5` together), `5ab67c2`, then `83f3b66` (inert) |

Must-land-together sets, all verified as blob chains or change/partial-revert pairs:
`59b9033`→`e4aa60d`→`f9b152d`→`6d7b21c`; `8a1cad0`→`afa767f`→`b67cf2e`;
`487168d`→`7e76bdb`→`7821dd3`; `6baa565`+`7a59369`; `d7c7734`→`d3be1bc`;
`cf102d4`+`f5c2b76`; `2ec9881`…`28f0733`; `072411b`+`1444fa0`;
`634be2b`+`44d7fa1`; `311d57e`+`c4ad9b5`.

### 11.7 Tooling this sync must fix (found while planning, not after)

1. **`just test` filters go stale the moment stage 8 lands.** The recipe names
   `actor::event_tap` and `actor::gesture_tap`; upstream deletes both modules. A
   filter that matches nothing is not an error to libtest — it is the *same silent
   failure* as the dead `--skip` §10.3 found, in the opposite direction. Replace
   with `actor::input` (which covers upstream's new `input::tests` **and**
   `input::gesture_tests`).
2. **The allowlist guard only checks one direction.**
   `just_test_allowlist_classifies_every_test_module` walks `src/` and asserts every
   test module is classified; nothing asserts every *filter* still names a real
   module. Add the mirror (`just_test_filters_all_name_a_real_module`) — it is the
   same walk, already written, compared the other way.
3. **`crates/` tests never run at all.** `just test` and `just test-all` both run
   `cargo test --lib`, which builds only the root lib. `crates/rift-protocol/src/transport.rs`
   arrives in this sync with a `#[cfg(test)] mod tests`, and the guard above cannot
   see it either (it walks `src/` only). Either add a workspace test recipe or
   extend the guard's walk to `crates/*/src`.
4. **New test modules to classify in stage order:** `actor::input::{tests,gesture_tests}`
   (stage 8 — covered once item 1 lands), `layout_engine::systems::floating::tests`
   (stage 7 — covered by the existing `layout_engine` filter),
   `sys::cocoa::tests` (stage 8 — falls under `sys` in `GUI_OR_DEFERRED`, but it is
   pure atomic-counter logic; **measure it** with
   `cargo test --lib -- sys::cocoa --test-threads=1` before accepting the exclusion,
   per the rule's own instruction).
5. **Re-verify the surviving `--skip`.** `e4aa60d` rewrote `identify_stale_windows`,
   the subject of `wsid_rekey_preserves_floating_membership_and_position`'s
   failure, but kept a predicate with no rekey suppression — so the skip probably
   survives. Settle it in a pristine worktree
   (`git worktree add /tmp/rift-up upstream/main && cargo test --lib wsid_rekey…`)
   rather than assuming: if it is green, delete the skip; if red, re-point the
   comment at the new predicate.
6. **`[[bin]] rift-cli test = false`.** Upstream deleted it (its rift-cli tests now
   run); our merge keeps it. `GUI_OR_DEFERRED` justifies excluding `bin::rift-cli`
   on the grounds that `--lib` never compiles it — still true, but the *reason*
   changes if we take upstream's removal. Decide explicitly in stage 9.

### 11.8 Live verification checklist (after stage 10)

The pattern from §4.7/§10.6: the gates prove it compiles and the logic tests pass;
only the live pass finds the rest. Minimum set, all on scratch windows:

- **Hint bar with `stack_line.enabled = false`** — click a column chip and a
  workspace pill. This is H6, and nothing else detects it.
- **Scroll speed** — a non-default `scroll_speed` still scales two-finger scrolling
  (proves the `4b460a0` port landed on `handle_scroll`, not on a dead struct field),
  and hot reload of that key still works through `Request::ConfigUpdated`.
- **§2.B activation** — `focus_window --activate` foregrounds; FFM hover does **not**
  foreground (H1 in the wrong direction is `activate: true`, which no test catches).
- **Post-wake raise** — sleep/wake, then focus a window on another workspace: with
  `6d7b21c` the raise self-heals instead of logging our `7ceb4d5` warn.
- **Off-screen parking (`cf102d4`)** — scroll a strip so columns leave the viewport;
  confirm no cross-app overlap at the edges, that hint-bar letters still match
  `focus-column N`, and that horizontal `move_focus` no longer jumps displays.
- **Floating layout** — `LayoutMode::Floating` on a scratch workspace, with an
  external bar configured: expect the §11.5 inset bug, and record it.
- **`rift --version`** under the nix build (expect `0.5.9+unknown.dirty` unless
  `build.rs` is patched).
- **Menu open** pauses mouse focus (`e9eafba`), and menu-bar rendering still tracks
  workspace switches after `dd9ad8f`'s render-skip cache.

### 11.9 Open questions (settle by doing, not by reading)

1. Does `wsid_rekey_preserves_floating_membership_and_position` still fail on
   upstream tip? (§11.7 item 5 — the only thing that can delete a skip.)
2. Should the fork enable `custom-event-loop`? Default is off, upstream calls it
   experimental, and §2.B's `ActivateIgnoringOtherApps` timing has only ever been
   validated under the AppKit pump. Recommendation: **no**, and revisit only with a
   measured reason.
3. Does the fork ever publish GitHub releases on `origin`? If not, the pre-existing
   homebrew-tap steps in `release.yml` are dead weight that will fail loudly the
   first time someone does — decide whether to guard them while §11.3 item 5 is open.
4. `RIFT_VERSION` under nix: env override in `nix/package.nix`, or accept
   `+unknown.dirty`?

### 11.10 Outcome — executed 2026-09-18, as planned

```
behind upstream           0        (was 77)
ahead                     78
upstream tip merged       83f3b66
curated tests             549 -> 605, plus 14 crate tests that had NEVER run
conflict hunks resolved   13 by hand, in 8 of the 10 stages
UD migrations             2 (event_tap.rs, gesture_tap.rs -> input.rs)
cargo check               --workspace --all-targets clean at every stage gate
just test / test-crates   green at every stage gate
just fmt-check            clean
```

| Stage | Checkpoint | Commits | Conflicts | Cost beyond the merge |
|---|---|---|---|---|
| 1 | `21efa52` | 10 | 1 | `handle_raises` union; the predicted inventory-token migration cost **nothing** |
| 2 | `372fcd2` | 4 | 1 | `activate: false` inside `focus_window_raise_request` |
| 3 | `251fe8b` | 8 | 2 | 16 `Column` literals swept; one hand brace; `actor::process::tests` allowlisted |
| 4 | `83830a7` | 12 | 1 | `release.yml` `@ours`; `docs.yml` guarded |
| 5 | `f2691fd` | 10 | 1 | trait-default union; a dropped `LayoutKind` import |
| 6 | `0121cf2` | 5 | 2 | `Quiet::Yes` + `activate` at both sites; new H2 guard test |
| 7 | `1628c1c` | 3 | **0** | proptest restated for parked columns; one `MoveFocusArgs` adaptation |
| 8 | `28f0733` | 10 | 5 + 2 UD | the input fold; new H6 gate + guard test; three filter/allowlist holes |
| 9 | `46e1ff5` | 7 | 1 | `tag.yml` guarded; two version lies fixed at the source |
| 10 | `83f3b66` | 8 | **0** | symbol-level verification only |

**Predictions that held.** All 16 `TAKE-REWORK`s were rework and nothing else was.
Every hazard H1–H8 was real and arrived exactly where §11.4 said. The `Column`
`node_id` sweep was 16 literals against a predicted "~15". The two `SKIP`s were
correct: both docs workflows reference `acsandmann/rift-docs` and a secret no fork
has.

**Predictions that were wrong, and in which direction.**

1. **§11.3 item 3 (the inventory-token migration) cost zero.** It was budgeted as
   "the largest chunk of cluster hours" — wide but mechanical, ~5 fork call sites.
   Git migrated all of them; `GetVisibleWindows` has no occurrences left in `src/`
   and `cargo check` was clean on the first try. The lesson is the same one §4.6
   recorded: *predicted* mechanical sweeps are the least reliable estimate in a
   sync plan, in both directions.
2. **Two silent losses the plan did not anticipate**, both from upstream's own
   cleanup rather than from a conflict: `b3e916e` dropped `scrolling.rs`'s
   `LayoutKind` import that our `group_containers()` needs (stage 5, E0433), and
   the stage-3 `engine.rs` union cut *inside* two function bodies, so `@both`
   produced an unclosed delimiter. Both loud, both cheap — but both are evidence
   for §4's "reconstruct the region, don't patch the markers".
3. **H6 needed new code, not a port.** The plan said to widen
   `desired_event_mask`; what it did not say is that this is the only place in the
   whole sync where a *faithful* port of fork code would still have been a
   regression. Worth stating plainly: porting a hunk to its exact destination is
   not the same as preserving its behaviour.
4. **The `0.5.9` bump was not a hand-bump.** §11.3 item 5 said to hand-bump
   `nix/package.nix:116`. That would have re-created the same skew next release, so
   `riftVersion` now reads `Cargo.toml` (`lib.importTOML`), and `build.rs` honours
   `RIFT_VERSION_OVERRIDE` so nix binaries stop reporting `+unknown.dirty`. Two
   version lies, both structural, both closed at the source.
5. **One skip died.** `wsid_rekey_preserves_floating_membership_and_position`
   passes on `upstream/main` (`83f3b66`) and here — `e4aa60d` repaired the
   `identify_stale_windows` predicate §9.2 bisected. `just test` now carries **one**
   skip. `topology_change_clears_stale_pending_hide_target` still fails upstream;
   its assertion message now names `RefreshWindowInventory`, so it is the same
   long-standing failure wearing the new pipeline's clothes.

**The three tooling holes closed themselves, in production.** §11.7's guards were
written *before* the merges, and every one of them fired on a real hole rather than
on a hypothetical:

- `just_test_filters_all_name_a_real_module` failed the instant stage 8 deleted
  `event_tap.rs`/`gesture_tap.rs` — the exact scenario it was written for, one
  commit after it was written.
- The same guard then caught `ipc` going dead: upstream moved those tests into
  `crates/rift-protocol`, which nothing ran until `just test-crates` existed.
- `just_test_allowlist_classifies_every_test_module` caught three arriving modules
  (`actor::process`, `actor::config`, `actor::reactor::mouse_focus_publisher_tests`)
  — takings its lifetime count to eight.

**Two new guard tests, both mutation-checked** (each fails when its fix is
reverted): `focus_window_command_raise_is_quiet_and_does_not_activate` (H2 — the
command-site quiet upstream's own test does not reach) and
`hints_bar_alone_keeps_mouse_button_delivery_in_the_mask` (H6). One fork property
was **restated rather than deleted**: `scrolling_columns_are_contiguous_and_non_overlapping`
now partitions visible from parked columns and asserts `cf102d4`'s parking
invariant on the latter — strictly more property than before.

**Verified beyond the gates.** Both binaries build and run; all four fork verbs
survive upstream's CLI restructure (`execute layout toggle-tabbed`,
`cycle-column-width`, `focus-column`, and `execute window focus --activate`);
`nix eval .#rift.version` reads `0.5.9`; `nix build .#rift` succeeds.

**Still open, deliberately.** The §11.8 live checklist is a *desktop* pass and has
not been run — hint-bar clicks with `stack_line` disabled (H6's live half), the
scroll-speed knob, FFM non-activation, post-wake raise self-heal, and the floating
layout's inset bug (`FORK.md` §8 feature item 6, created by this sync). Nothing in
the checklist is testable headless, which is exactly why §11.4 lists "no test
covers this" against six of the twelve hazards.
