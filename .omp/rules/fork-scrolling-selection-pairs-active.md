---
description: "In the scrolling layout, a `state.selected` write must pair with the owning column's `active` (§2.D tab memory)"
astCondition: "state.selected = Some($W)"
scope: "tool:edit(*scrolling.rs), tool:write(*scrolling.rs)"
interruptMode: tool-only
probes:
  fire:
    - "state.selected = Some(new_sel);"
    - "state.selected = Some(column.windows[target_idx]);"
  silent:
    # Reading the selection is not a write.
    - "let sel = state.selected;"
    - "state.center_override_window = None;"
---

# Pair the selection write with the column's `active`

Our §2.D tabbed-columns feature stores a per-column last-focused tab in
`Column.active`. A `Column` shows that tab, not its row 0 — so a `state.selected`
write that does not update the owning column's `active` leaves tab memory pointing
at a window the user never left on.

```rust
// Paired — the invariant.
state.selected = Some(new_sel);
state.columns[col_idx].active = Some(new_sel);
```

Established pairings: `scrolling.rs` `move_focus_vertical`, `move_focus_horizontal`,
`focus_column`, and `snap_to_nearest_column`.

## Where an unpaired write is legitimate

Not every write pairs. Removal/extraction paths deliberately **clear** stale memory
instead, and single-window column paths cannot drift. If the write you are making is
one of those, say so in a comment and move on — this rule is a prompt to decide, not
a prohibition.

## Why this exists

Upstream's `9215d53` rewrote `snap_to_nearest_column` to also *select* a window (it
previously only scrolled), indexing the target column by the **source** column's row
and never touching `active`. The merge was textually clean, it compiled, and all 547
tests passed — the feature was silently broken in two ways at once. Guarded now by
`snap_restores_the_target_columns_active_tab` and
`snap_pairs_tab_memory_with_the_selection_on_a_plain_stack`, both mutation-checked.

A clean auto-merge is not a safe auto-merge. When upstream **rewrites** a file we
diverge in, budget review time per rewritten file, not per conflict hunk.
