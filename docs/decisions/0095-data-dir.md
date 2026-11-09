# 0095 Data dir

- Status: decided
- Kind: product
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
Where does the data dir live?

## Facts
Verified 2026-10-07.
- Pi 1.0.1 keeps sessions in `<agent dir>/sessions` (`dist/config.js:489-491`); omp v18.6.0 in `~/.omp/agent/sessions` (`packages/utils/src/dirs.ts:955-957`); opencode v1.18.22 uses the XDG data dir on every OS (`packages/core/src/global.ts:11`).
- `%LocalAppData%` is Windows' per-machine, non-roaming location.
- Phase 0 keeps sessions in `<config dir>/sessions` until phase 1 (0057).

## Options
| Option | Pros | Cons |
|---|---|---|
| OS data dirs: `%LocalAppData%\jarvis\data`, `~/Library/Application Support/jarvis/data`, `$XDG_DATA_HOME/jarvis/data` | OS convention; machine-written data apart from hand-edited config; non-roaming on Windows | Two roots to know |
| `~/.jarvis/data` | Everything under `~/.jarvis`; one path on every OS | Not the OS convention; home backups pick up large DBs |
| Inside the config dir, `~/.jarvis/agent/data` | One root, like Pi and omp | Machine data inside Pi's config tree |

## Recommendation
The OS data dirs.

## Choice
The OS data dirs: `%LocalAppData%\jarvis\data` on Windows, `~/Library/Application Support/jarvis/data` on macOS, `${XDG_DATA_HOME:-~/.local/share}/jarvis/data` on Linux.

## Reason
Not given.

## Consequences
- SQLite DBs and per-thread Pi sessions (1.5) live under the data dir; phase 0's `<config dir>/sessions` is left behind (0057).
