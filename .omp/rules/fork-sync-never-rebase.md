---
description: "Upstream syncs are merges, never rebases — a rebase re-resolves every fork conflict once per upstream commit"
# `git\s+rebase` was too tight: `git -C <dir> rebase upstream/main` slipped through,
# and scratch worktrees are exactly where a sync gets experimented on.
condition: "\\bgit\\b[^\\n;&|]{0,40}\\brebase\\b[^\\n;&|]{0,80}\\bupstream\\b"
scope: "tool:bash"
interruptMode: tool-only
probes:
  fire:
    - "git rebase upstream/main"
    - "git rebase --onto upstream/main HEAD~3"
    - "git -C /tmp/probe rebase upstream/main"
    # A `-c` prefixed rebase is still a rebase.
    - "git -c rerere.enabled=false rebase upstream/main"
  silent:
    - "git merge upstream/main"
    - "just sync"
    - "just sync-to 7829780"
    # Rebasing our own local commits is unrelated to absorbing upstream.
    - "git rebase -i HEAD~3"
---

# Sync with a merge, not a rebase

`UPSTREAM-SYNC.md` §3 rejected rebasing on measured grounds; §9.4 added a second,
independent reason.

**A rebase replays our ~60 fork commits against every upstream step**, re-resolving
the same conflict once per upstream commit. A merge resolves each conflict **once**,
and `rerere` records it for next time.

```bash
# WRONG — re-resolves our most delicate feature N times.
git rebase upstream/main

# RIGHT — merges the whole range, gated on build + tests.
just sync
```

## Why the whole range, not commit-by-commit

Two traps, both measured in real syncs:

- **Partial-revert pairs.** On `b31dddf..7829780` a range merge cost **2** additive
  hunks. Commit-by-commit, `1b69d8e` alone conflicts across **7 files** and rewrites
  `Request::Raise` to carry `FocusConfirmation` where our fork carries
  `activate: bool` — then `7829780` reverts the whole thing. You would resolve the
  fork's most delicate seam across 7 files to arrive exactly where you started.
- **Commits that do not compile alone.** `fad6498` adds a test calling a helper only
  `5262221` adds, 25 seconds later. A per-commit build gate breaks there through no
  fault of the merge.

Too large for one bite? `just sync-to <sha>` — a *smaller range*, still a range.

## Rebase is for upstreaming only

A clean linear PR against `acsandmann/rift` is the one place it earns its cost, and
that happens on a dedicated feature branch, never on `xieyt/5`.
