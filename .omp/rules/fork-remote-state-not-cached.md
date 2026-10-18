---
description: "`origin/<branch>` is a local cache; confirm with `ls-remote` before reporting push state"
condition: "rev-list[^\\n]{0,60}\\borigin\\b"
scope: "tool:bash"
interruptMode: tool-only
probes:
  fire:
    - "git rev-list --count origin/develop..HEAD"
    - "git rev-list --left-right --count origin/develop...HEAD"
  silent:
    # upstream/main is refreshed by the `git fetch upstream` these recipes run.
    - "git rev-list --count HEAD..upstream/main"
    - "git rev-list --reverse --no-merges HEAD..upstream/main"
---

# `origin/<branch>` is a cache, not the remote

`git rev-list --count origin/develop..HEAD` reads the **local remote-tracking ref**.
That ref only moves on `git fetch origin` or `git push`. A `git fetch upstream` does
**not** refresh it.

On 2026-08-28 this produced a confidently wrong report — first "145 commits
unpushed", then "local is 2 commits ahead of origin/develop" — while the remote was
already at our exact tip. The `git push` that followed was a no-op.

## Confirm before reporting

```bash
# Authoritative — asks the remote.
git ls-remote origin refs/heads/develop | cut -f1
git rev-parse HEAD

# Or refresh the cache first; then the cached ref is trustworthy.
git fetch origin --quiet && git rev-list --count origin/develop..HEAD
```

## `origin/HEAD` is the same trap

On 2026-10-18 the cached `origin/HEAD` still said `main` after the remote default
branch had become `develop` and `refs/heads/main` had been **deleted**. Nothing
refreshes a symref except `git remote set-head origin -a`. Ask the remote:

```bash
git ls-remote --symref origin HEAD
```

Our work lives on `origin/develop`. There is no `origin/main` and no
`origin/xieyt/5`; both names in older notes mean `develop` (`FORK.md` §1).

## The general form

This is the sixth instance of one pattern here: **a cached or indirect reading
treated as ground truth.** The others: `alacritty --title` (silently ignores OSC
title sequences), a `focus-column` check racing a new window's own focus grab, a
ghost-window probe reading `pid` from the wrong JSON nesting, and `container_tree`
reporting a tabbed column as `Vertical`.

When a check disagrees with the code, **suspect the probe first** — and name the
instrument when you report the result.
