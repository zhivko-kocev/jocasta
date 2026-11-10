# 0468 `jarvis uninstall`

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
0446's removal list is written by hand. How does Jarvis remove itself so the list can't fall behind what install and setup add?

## Facts
Verified 2026-10-23.
- 0446's removal list is hand-written: install folder, PATH, autostart, herdr session, containers, images and volumes, the data folder, `~/.jarvis/agent`, the MCP entries, herdr's omp extension, the Terminal scheme, the calendar credential; omp `config.yml` is kept.
- Omarchy pairs every optional `install-*` command with a `remove-*` command (46 `install-*`, 34 `remove-*`, plus `webapp-install`/`-remove`), so removal can't drift (https://github.com/omacom/omarchy/tree/quattro/bin).

## Options
| Option | Pros | Cons |
|---|---|---|
| Each install and setup step records its undo; `jarvis uninstall` runs them | Removal stays complete as setup grows | Every step needs an undo |
| Keep 0446's hand-written list | Already written | Drifts as setup grows |

## Recommendation
Each install and setup step records its undo; `jarvis uninstall` runs them.

## Choice
`jarvis uninstall` removes exactly what install and setup added: each install and setup step records how to undo itself (the install folder, PATH entry, autostart value, herdr session `jarvis`, the omp MCP entries, herdr's omp extension, the Terminal fragment, the calendar credential, `~/.jarvis/agent` except his own `private-names.txt`, which the repo scan reads). It asks separately before removing the data folder (memory, models) and before removing the containers, images and volumes. Keys setup merged into his omp `config.yml` stay (they are his settings). The v1 cleanup (0446) runs through it.

## Reason
"`jarvis uninstall` removes exactly what install and setup added".

## Consequences
- Amends 0446: its cleanup is `jarvis uninstall`, not a hand-written list.
- Keys merged by 0466 stay in his `config.yml`.
- mise itself (installed for 0476 by its script, not by Jarvis's installer) isn't removed by `jarvis uninstall`; the v1 cleanup lists it: `~/.local/bin/mise.exe` and `mise-shim.exe`, `~/.local/state/mise/`, `%TEMP%\mise\`, and the `%LOCALAPPDATA%\mise\shims` Path entry (the install also changed the user Path's registry type from REG_SZ to REG_EXPAND_SZ).
- Amended by 0595: the Jarvis on his PC predates this command, so the cutover ran 0446's list by hand.
