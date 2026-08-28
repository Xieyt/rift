---
description: "A new `#[cfg(test)]` module must be classified in the `just test` allowlist or it silently never runs"
condition: "#\\[cfg\\(test\\)\\]"
scope: "tool:edit(*.rs), tool:write(*.rs)"
interruptMode: tool-only
probes:
  fire:
    - "#[cfg(test)]\nmod tests {"
    - "#[cfg(test)]\npub(crate) mod fixtures {"
  silent:
    - "#[test]\nfn selection_survives_a_rekey() {}"
    - "#[cfg_attr(test, derive(Debug))]\nstruct Column {}"
---

# A new test module must be classified, or it silently never runs

`just test` runs a **curated allowlist** of module filters rather than the whole
suite (`sys` genuinely aborts headless). A curated allowlist fails *silent*: adding
a `#[cfg(test)] mod tests` without adding its filter is a no-op that looks exactly
like coverage.

Every new test module goes in **one** of two places:

1. the `test:` recipe's filter list in `justfile` — if it passes headless; or
2. `GUI_OR_DEFERRED` in `common::config`'s
   `just_test_allowlist_classifies_every_test_module` — if it genuinely needs a GUI,
   or declares only helpers/fixtures with no `#[test]`.

Measure before choosing (2). Do not assume:

```bash
cargo test --lib -- <module> --test-threads=1
```

That assumption was wrong for **13 modules**: 148 tests that pass perfectly well
headless were not running, including `model` (70) and `actor::spaces` (42).
`ui::hints_bar` was missing for months while `FORK.md` §2.E claimed those tests ran.

## The guard catches this, but later

`just_test_allowlist_classifies_every_test_module` fails **by name** when a module is
in neither list. It has paid off five times, most recently on
`actor::gesture_tap::tests` arriving from upstream — so this rots on *upstream's*
schedule, without anyone touching our code. Classifying as you write is cheaper than
a red suite after a merge.
