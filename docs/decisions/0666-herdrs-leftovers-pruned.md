# 0666 herdr's leftovers pruned, the menu and help in Jocasta's words

- Status: v1
- Kind: product, ux
- Phase/slice: fork build, after his first day on it
- Date: 2026-11-23

## Question
With 0665, which of herdr's remaining client features that speak for herdr rather than Jocasta go?

## Facts
Verified 2026-11-23 in the fork (`crates/herdr/src/client/shell/`):
- The global menu (`global_menu.rs`) offers settings, keybinds, reload config, "what's new" / "update ready" (herdr's release notes, `release_notes.rs`; the fork has no updater, 0649) and "detach" (also Ctrl+B q). Quitting Jocasta is `jocasta.quit` (Ctrl+B Alt+Q, 0563).
- Corrected the same day, after the first question: "detach" doesn't leave Jocasta running. When herdr's last client leaves, the service runs its close (`embedded.rs` `last_client_left`, `server.rs` `client_closed`): everything stops (0549), asked first from a new terminal while threads work or await him. "detach" is a second quit.
- herdr's product announcement overlay and its first-run onboarding are still in the code; `config/herdr.toml` sets `onboarding = false`.
- herdr's settings have an Integrations tab for installing agent integrations; Jocasta's setup installs omp's (0650).
- Help (Ctrl+B ?) lists herdr's keys only; Jocasta's are the `[[keys.command]]` entries in `config/herdr.toml`.

## Options
One tick each, recommended on: release notes and "what's new" / "update ready" out; announcements out; onboarding out; the Integrations tab out; "detach" reworded with "quit Jocasta" beside it; help with Jocasta's keys first. After the correction, for leaving: "quit Jocasta" only (recommended); "quit Jocasta" with "detach" renamed "close this window (Jocasta asks before stopping threads)"; or a real hide that keeps Jocasta running windowless, which would change 0549.

## Recommendation
All six.

## Choice
His answers of 2026-11-23: all six, then "quit Jocasta" only. herdr's release notes, its menu items, the announcement overlay, onboarding and the settings' Integrations tab are removed from the fork. The menu's one way out is "quit Jocasta", which runs `jocasta.quit` as Ctrl+B Alt+Q does; "detach" and its key go. Help lists Jocasta's keys first, then herdr's.

## Reason
The surface speaks for Jocasta; what herdr says about itself has no place in the fork.

## Consequences
- Amends 0649; `crates/herdr/FORK.md` lists the removals.
- `onboarding` and a detach key binding in a `herdr.toml` are read and ignored, so an old config still loads; `config/herdr.toml` drops `onboarding`.
- Closing the terminal still closes Jocasta as 0549 says.
