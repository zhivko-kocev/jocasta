# 0550 omp stops updating itself until the benchmarks end, and setup's omp files are written on his PC now

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-08

## Question
A check of every setting before the spikes found his omp updating itself and setup's omp files missing on his PC. What is done now?

## Facts
Verified 2026-11-08 on his PC.
- Every key Jocasta ships in its omp YAML is still registered in omp 18.18.2 with the same type and default.
- His `~/.omp/agent/config.yml` has `update.auto: true` (omp 18.18.2 default false); that's how 18.18.2 arrived. 0535 pins the omp of the build's start, and each bump re-checks the seams (0383).
- His PC runs the release from before the rename (`~/.jarvis/agent`, herdr session `jarvis`); `~/.jocasta` doesn't exist, as the cutover (0487) hasn't run. `~/.omp/agent` lacks setup's `keybindings.yml` (`app.live.toggle: []`; without it Ctrl+L starts omp's live voice, which sends mic audio and the session to OpenAI), `WATCHDOG.md` and `lsp.json` (`idleTimeoutMs: 300000`) (0451, 0479).
- His live `herdr.toml` refreshes the HUD every 2 s; 0437 ships 5 s.
- Installed against the pins (2026-11-08): herdr 0.9.5, gh 2.92.0 and electrobun 2.0.2 (in `node_modules`) match; the `bun` on his PATH is 1.4.4 and `node` 24.13.0, while `package.json`, `mise.toml` and `mise.lock` pin Bun 1.4.3 and Node 24.21.0, which `mise run` and `mise exec` use (installed into mise's cache by this check; `mise run check` stays the way to run the gates); `hunk` isn't on his PATH, because setup's global mise tools (herdr, hunk, gh; `src/setup/mise.ts`) come with the cutover, so his current release skips the review step.

## Options
| Item | Options (pros / cons) |
|---|---|
| omp updates | Off until the benchmarks end (each bump deliberate) · Keep auto (omp may change between spikes, build and runs) |
| Setup's omp files | Write them now, shown first (the live voice key closes today) · At the cutover |

## Recommendation
Off; write them now.

## Choice
- `update.auto: false` in his `~/.omp/agent/config.yml` until the benchmarks end; bumps are deliberate, each with 0383's seam check.
- Written on 2026-11-08 after he saw them: `~/.omp/agent/keybindings.yml` (`app.live.toggle: []`), `~/.omp/agent/lsp.json` (`{ "idleTimeoutMs": 300000 }`) and `~/.omp/agent/WATCHDOG.md` (a copy of `config/WATCHDOG.md`). `omp config get update.auto` reads `false`.

## Reason
"Off until the benchmarks end"; "Write them now", then "Yes, all four" to the shown changes.

## Consequences
- Written by hand, these carry no setup marker: at the cutover `jocasta setup` finds its values already there, and uninstall leaves them as his.
- The HUD interval and the rest of his live files change at the cutover, when setup runs (0487).
