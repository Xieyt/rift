---
description: "This fork never pushes to `upstream` (`acsandmann/rift`) — `origin` only"
# Gaps exclude `;`, `&`, `|` so a compound command cannot bridge an innocent
# `git push origin …` into an unrelated `… upstream` clause and burn the rule
# (repeatMode is `once`). `\bgit\b[^…]{0,40}\bpush\b` tolerates `-C <dir>` / `-c k=v`.
condition: "\\bgit\\b[^\\n;&|]{0,40}\\bpush\\b[^\\n;&|]{0,80}\\bupstream\\b"
scope: "tool:bash"
interruptMode: tool-only
probes:
  fire:
    - "git push upstream xieyt/5"
    - "git push --force-with-lease upstream main"
    # Pushing *to* origin but writing an upstream ref is the same mistake.
    - "git push origin upstream/main:main"
    - "git -C /tmp/probe push upstream main"
  silent:
    - "git push origin xieyt/5"
    - "git fetch upstream --quiet"
    # `upstream/main` as a read source is fine; only `push` is the violation.
    - "git merge upstream/main"
    - "git rev-list --count HEAD..upstream/main"
    # A compound whose push targets origin and whose *other* clause reads upstream.
    - "git push origin xieyt/5 && git fetch upstream --quiet"
---

# Never push to `upstream`

`upstream` is `acsandmann/rift`, a repository we do not own. `FORK.md` §1 states the
rule as absolute: **local work never touches upstream. We only ever push to
`origin`** (`xieyt/rift`).

```bash
# WRONG — pushes our fork's private work into someone else's repository.
git push upstream xieyt/5

# RIGHT — our fork, our branch.
git push origin xieyt/5
```

`upstream` is fetch-only in practice, even though the remote has a push URL
configured. No workflow in this repo legitimately pushes there.

## The one exception, and it is not this

Contributing back is done by opening a PR **from a dedicated feature branch on
`origin`**, never by pushing to `upstream`. `FORK.md` §1: "The original developer
only sees what we deliberately open as a PR from a dedicated feature branch."

If you are upstreaming: `git push origin <feature-branch>`, then the `github` tool
(`op: pr_create`, `repo: acsandmann/rift`).

## Before you retry

If the intent was to *read* upstream, the verb is `fetch`, not `push`.
