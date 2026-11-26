# 0692 Unix install folder

- Status: v1 (built 2026-11-26)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J1 (#2)
- Date: 2026-11-26
- Amends: 0049 (the Unix install folder)

## Question
On Linux the default install folder `~/.local/share/jocasta` holds the default data folder `~/.local/share/jocasta/data` (0049 with 0095), so `install.sh`'s upgrade swap (0579) moves the data aside with the old build and removes it, and a run before the first install leaves a folder `install.sh` then refuses. Where does `scripts/install.sh` install by default?

## Facts
- `scripts/install.sh` defaulted to `$HOME/.local/share/jocasta`; `src/shared/paths.rs` puts Linux's data at `${XDG_DATA_HOME:-~/.local/share}/jocasta/data`.
- The swap ends in `rm -rf` of the old folder; data inside it goes with it (issue #2).
- ultron already installs Jocasta into `~/.local/lib/jocasta` through `JOCASTA_INSTALL_DIR`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Default to `~/.local/lib/jocasta` | Program and data never nest; no special case in the swap | Installs made with the old default stay where they are until reinstalled |
| Keep the folder and carry `data/` across the swap | No move for old installs | The swap learns about a folder that isn't the release's; still nested |

## Recommendation
`~/.local/lib/jocasta`.

## Choice
His answers of 2026-11-26 and the approved plan: `scripts/install.sh` defaults to `~/.local/lib/jocasta` on Linux and macOS, outside the data folder; `JOCASTA_INSTALL_DIR` still overrides it. README and manual say so.

## Reason
Program and data belong in separate folders; the upgrade then never touches his threads, journal or notification log.

## Consequences
- `scripts/install.sh`'s default, its header comment, README's install section and the manual's Linux and macOS paragraph change.
- An install under the old default keeps working; installing again without `JOCASTA_INSTALL_DIR` puts a second copy in the new folder.
