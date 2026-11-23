# 0659 The sidebar without herdr's spaces section

- Status: v1
- Kind: product
- Phase/slice: fork build, after his first day on it
- Date: 2026-11-23

## Question
On his first day on the Rust build he said: "the left side the spaces panel is not needed as we are using one space to work in. the Jocasta space. agents/threads i think it is okay to stay." Hide it by a setting, remove it, or keep it?

## Facts
Verified 2026-11-23:
- herdr's expanded sidebar draws two sections: " spaces" (the workspace list) on top and the agents list below (`crates/herdr/src/client/shell/sidebar.rs` `render_sidebar`). The split ratio is clamped to 0.1–0.9 with at least 3 rows each (`crates/herdr/src/ui/sidebar.rs` `sidebar_section_heights`). No config key hides the top section.
- A thread is its own herdr workspace (0412), so each thread shows in both lists. The agents list is Jocasta's "threads" view (`native.rs`); a click focuses a thread's pane, and Alt+[ / Alt+] and Alt+1..9 move between workspaces without the list.

## Options
- **A `hidden` switch in `[ui.sidebar.spaces]`, on in Jocasta's `config/herdr.toml`.** Pros: reversible by config. Cons: a new config key and both layouts to keep working.
- **Remove the section from the fork.** Pros: one layout, the whole sidebar for the agents list, less code. Cons: no way back without a fork change.
- **Keep it.** Pros: nothing to build. Cons: the duplicate list stays.

## Recommendation
The switch.

## Choice
His answer of 2026-11-23: "remove it in the project directly i guess that will also free space for better ui". The fork's expanded sidebar draws no spaces section: the agents list takes its whole height, with no section divider to drag. The section's code (its rows, hits, scroll, divider, and keyboard and mouse handling) goes with it; workspaces stay, reached by the agents list, Alt+[ / Alt+] and Alt+1..9. The collapsed sidebar's workspace markers stay.

## Reason
His words above: one space to work in, and room for the threads.

## Consequences
- `crates/herdr/FORK.md` lists the removal.
- Settings for the removed section in a `herdr.toml` (the split ratio, `[ui.sidebar.spaces]`) are read and ignored, so an old config still loads.
- Amends 0649 (what the fork keeps of herdr's client).
