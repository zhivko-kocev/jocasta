# 0660 A click on a `today` row acts like Enter

- Status: v1
- Kind: product, ux
- Phase/slice: fork build, after his first day on it
- Date: 2026-11-23

## Question
On his first day on the Rust build "the links to the news arent opening". How does a news link open?

## Facts
Verified 2026-11-23:
- Under 0581 (UX-7 A) a link row opens with ↑↓ to it and then Enter (`link.open`). In the dock a left click only gives the dock his keys (`crates/herdr/src/client/shell/extension.rs` `extension_takes_mouse`), and the pick starts on the first thread.
- That day the service was hung (ledger row 107), so Enter's `link.open` was never answered either.

## Options
- **A click on a row acts like Enter on it**: a link row opens in his browser, a thread row focuses its pane. Pros: what he tried first. Cons: the dock needs to know which row is under the mouse.
- **Keys only.** Pros: nothing to build. Cons: what failed him.

## Recommendation
The click.

## Choice
His answer of 2026-11-23: the click. A left click on a pickable `today` row picks it and does what Enter does on it: a link row opens (the same `link.open` and the same checks as 0581), a thread row focuses its pane. A click elsewhere in the dock only gives it his keys, as before.

## Reason
His report above.

## Consequences
- Amends 0581 (UX-7: Enter only).
- The key line stays as 0581 worded it.
