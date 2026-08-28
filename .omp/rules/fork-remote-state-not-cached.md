---
description: "`origin/<branch>` is a local cache; confirm with `ls-remote` before reporting push state"
condition: "rev-list[^\\n]{0,60}\\borigin\\b"
scope: "tool:bash"
interruptMode: tool-only
probes:
  fire:
    - "git rev-list --count origin/xieyt/5..HEAD"
    - "git rev-list --left-right --count origin/main...HEAD"
  silent:
    # upstream/main is refreshed by the `git fetch upstream` these recipes run.
    - "git rev-list --count HEAD..upstream/main"
    - "git rev-list --reverse --no-merges HEAD..upstream/main"
---

# `origin/<branch>` is a cache, not the remote

`git rev-list --count origin/xieyt/5..HEAD` reads the **local remote-tracking ref**.
That ref only moves on `git fetch origin` or `git push`. A `git fetch upstream` does
**not** refresh it.

On 2026-08-28 this produced a confidently wrong report — first "145 commits
unpushed", then "local is 2 commits ahead of origin/xieyt/5" — while the remote was
already at our exact tip. The `git push` that followed was a no-op.

## Confirm before reporting

```bash
# Authoritative — asks the remote.
git ls-remote origin refs/heads/xieyt/5 | cut -f1
git rev-parse HEAD

# Or refresh the cache first; then the cached ref is trustworthy.
git fetch origin --quiet && git rev-list --count origin/xieyt/5..HEAD
```

## Compare against the right branch

`origin/main` is a **clean mirror of upstream** in this fork — no local work lands
there (`FORK.md` §1). Measuring `xieyt/5` against `origin/main` yields a large,
meaningless number. Our work lives on `origin/xieyt/5`.

## The general form

This is the fifth instance of one pattern here: **a cached or indirect reading
treated as ground truth.** The others: `alacritty --title` (silently ignores OSC
title sequences), a `focus-column` check racing a new window's own focus grab, a
ghost-window probe reading `pid` from the wrong JSON nesting, and `container_tree`
reporting a tabbed column as `Vertical`.

When a check disagrees with the code, **suspect the probe first** — and name the
instrument when you report the result.
