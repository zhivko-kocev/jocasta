# 0670 Leftover worktrees in the dock, removed behind his Allow

- Status: v1
- Kind: product, ux
- Phase/slice: HUD and orchestration round, after the fork build
- Date: 2026-11-23

## Question
Of the additions beyond the dock he ticked: "Leftover worktrees in the dock, with one-key removal behind your Allow".

## Facts
Verified 2026-11-23: the service already sends `Dashboard.leftovers` (thread worktrees left on disk, 0456) to the dock, which doesn't draw them; `worktree_remove` removes one behind his Allow (0484, 0544).

## Options
- **Draw them, with a key that asks to remove the picked one.** Pros: the data is there; the Allow stays. Cons: a section more when there are leftovers.
- **Leave them to the boss.**

## Recommendation
Draw them with the key.

## Choice
His answer of 2026-11-23: a LEFTOVERS section, shown only when there are any, one line per worktree (project and folder); `x` on a picked row runs `worktree_remove` for it, which asks his Allow as it does today.

## Reason
Stale worktrees are a known chore in every orchestrator; seeing them is half the cleanup.

## Consequences
- Amends 0555 (the dock's sections) and 0581 (its keys).
