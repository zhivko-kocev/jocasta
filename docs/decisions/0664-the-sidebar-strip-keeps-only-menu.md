# 0664 The sidebar's top strip keeps only `menu`

- Status: v1
- Kind: product, ux
- Phase/slice: fork build, after his first day on it
- Date: 2026-11-23

## Question
After 0659's build he said: "we can also delete the new button in the left corner because if i click it i lose the jocasta boss or idk we make something else more understandable for our fork". What replaces the `new` button?

## Facts
Verified 2026-11-23 in the fork:
- The expanded sidebar's top strip draws `new · Local` and `menu` (`crates/herdr/src/client/shell/endpoint_sidebar.rs`). A click on `new` is herdr's `NewWorkspace` (`client/shell/mouse.rs` → `client/shell/actions.rs`): `workspace.create` with `focus: true`, a plain shell workspace in the focused pane's folder.
- Focus leaves "Jocasta", so the `today` dock (drawn only beside the boss, `src/frontier/shell.rs` `docks`) goes with it; the boss keeps running, reached again by Alt+1, Alt+g or Ctrl+B w.
- Jocasta's own new-thread form is Ctrl+B Alt+N (`jocasta.new-thread`, 0554), or asking the boss.
- `menu` opens herdr's global menu (settings, keybinds, reload config, detach; 0666 changes its items).

## Options
- **`new thread`, opening Jocasta's new-thread form.** Pros: the word means what it does in Jocasta. Cons: a second way to the form.
- **Two buttons, `jocasta` (back to the boss) and `new thread`.** Pros: a way home always visible. Cons: one more target.
- **Nothing; only `menu` stays.** Pros: the cleanest strip. Cons: a new thread only by key or by asking the boss.

## Recommendation
`new thread`.

## Choice
His answer of 2026-11-23: nothing; only `menu` stays. The strip draws `menu` alone, and a click where `new` was does nothing.

## Reason
His choice: new threads come from the boss or the key, and nothing on screen makes a workspace Jocasta doesn't know.

## Consequences
- Amends 0659 (whose build kept the `new` / `menu` strip).
- herdr's `NewWorkspace` key goes too, with the rest of herdr's workspace creation (0665).
- `config/herdr.toml` drops its `[ui.sidebar.spaces]` block, read and ignored since 0659.
