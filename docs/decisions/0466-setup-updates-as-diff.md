# 0466 Setup brings updates as a diff, applied after his y

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
`jarvis setup` writes omp's `config.yml` only when it is missing, so settings Jarvis ships later never reach his existing file. How do changes to the files setup owns reach a machine that already ran it?

## Facts
Verified 2026-10-23.
- Setup writes omp's `config.yml` only when missing (`src/setup/setup.ts:240-244`), so the new keys of 0447–0465 never reach an existing file.
- 0440 (2) already shows herdr.toml's diff and replaces it after `y` (`src/setup/setup.ts:267-280`).
- Jarvis's SQLite already uses numbered migrations (`src/shared/db.ts`, `user_version`).
- Omarchy keeps `migrations/<unix-ts>.sh`, run by `omarchy-migrate` with a marker per migration under `~/.local/state/omarchy/migrations/`; a failed one stays pending and stops the queue; `--pending` lists what is missing, and a login toast points to it, never running them silently (https://github.com/omacom/omarchy/blob/quattro/bin/omarchy-migrate, https://github.com/omacom/omarchy/blob/quattro/agents/skills/migrations.md).
- Omarchy's herdr migration changes a value only while it still equals what Omarchy shipped (https://github.com/omacom/omarchy/blob/quattro/migrations/1791722809.sh).
- A shipped omp `--config` overlay would rank above his own `config.yml` (omp `config/settings.ts:880-891`, `config/registry.ts:454-455`), so it can't serve as a default layer under his settings.

## Options
| Option | Pros | Cons |
|---|---|---|
| Numbered migrations shown as a diff, applied after `y`, with a start-up toast | New settings reach existing files; nothing changes without him; one runs once | A migration file per change |
| Keep write-when-missing | No work | His files never get new settings |
| Ship defaults as an omp `--config` overlay | No edits to his file | Overrides his own settings |

## Recommendation
Numbered migrations shown as a diff, applied after `y`, with a start-up toast.

## Choice
`jarvis setup` shows every pending change to the files it owns as a diff and applies it only after his `y`: herdr.toml, the omp MCP entries, the Windows Terminal fragment, and the omp settings Jarvis ships in `config/omp-config.yml` — merged into his existing `config.yml` key by key (keys he set to another value are shown, never overwritten silently; his other keys are untouched). Changes are numbered migrations with a marker each, so one runs once; a failed one stops the rest and says why. The companion runs setup's read-only check at start and shows one toast when changes are waiting; nothing changes until he runs setup.

## Reason
"Updates shown as a diff, applied only after your y" and "One toast when setup has changes waiting".

## Consequences
- Amends 0420 (the reference config now reaches his existing `config.yml`, key by key, after his `y`) and 0440 (herdr.toml's replacement becomes one of the numbered migrations).
- The migration folder, marker location and toast text are chosen by the builder and listed as built; plan appendix I names them.
- Amended by 0476: setup also writes `~/.config/mise/conf.d/jarvis.toml` when mise is installed.
- Amended by 0479: three more migrations for `keybindings.yml`, `WATCHDOG.md` and `lsp.json` in his omp folder.
