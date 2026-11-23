# 0667 The boss's row pinned first in the sidebar

- Status: v1
- Kind: product, ux
- Phase/slice: HUD and orchestration round, after the fork build
- Date: 2026-11-23

## Question
He asked for "a tab of the boss so i can go back now if i switch to a thread i cant go back to jocasta". How does he get back to the boss from a thread?

## Facts
Verified 2026-11-23:
- The sidebar lists the agents through Jocasta's Agents view (`src/frontier/server/native.rs` `agent_view`), whose filter leaves the boss's workspace out (0553, 0554), so the boss has no row.
- The boss is reached only by keys: Alt+1 (`config/herdr.toml` `switch_workspace`), Alt+G or Ctrl+B g (`goto`), Ctrl+B w.
- 0664 left the top strip with `menu` only; one option there was a `jocasta` button, not chosen.

## Options
- **A "Jocasta" row pinned first in the sidebar's list, with the boss's state.** Pros: where the threads are, like a tab; shows when the boss is working or waits for him. Cons: one more row.
- **A `jocasta` button beside `menu`.** Pros: small. Cons: no state; gone when the sidebar is collapsed.
- **Both.**

## Recommendation
The pinned row.

## Choice
His answer of 2026-11-23: the pinned row. The sidebar's list starts with a "Jocasta" row for the boss's pane, with its state (working, blocked, idle) as the threads' rows show theirs; a click or Enter on it focuses the boss's workspace. It stays first whatever the sort, and the dock keeps leaving the boss out of its own lists.

## Reason
A way home always in sight, without a second strip button.

## Consequences
- Amends 0553 and 0554 (the Agents view's filter) and 0664 (the sidebar's way back to the boss).
- Alt+1, Alt+G and Ctrl+B w stay.
