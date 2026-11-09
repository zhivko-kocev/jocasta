# 0476 mise as the single entry for development

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
He handles development tools by hand today (Bun, herdr, hunk, gh, Node and pnpm for the build) and wants one entry for development. Does Jarvis use mise for that, and how far?

## Facts
Verified 2026-10-23 (`local://mise-single-entry.md`).
- On Windows x64 mise can pin `bun`, `node`, `pnpm`, `herdr` (aqua, Windows zips after 0.8.0), `hunk`, `gh` and omp (`oh-my-pi` standalone exe, or `npm:@oh-my-pi/pi-coding-agent`); Jarvis's build needs no Rust (nothing in `scripts/build.ts` calls cargo).
- A Jarvis-owned machine file fits at `%USERPROFILE%\.config\mise\conf.d\jarvis.toml`; his own `config.toml` wins on conflicts; `MISE_GLOBAL_CONFIG_FILE` would hide his config (https://mise.jdx.dev/configuration.html).
- A repo `mise.toml` with only tools and tasks needs no trust; tasks run through `cmd /c` on Windows (mise docs, "What needs trust", `windows_default_inline_shell_args`).
- Privacy: `use_versions_host` and `use_versions_host_track` send tool ids, versions, OS and IP (kept 35 days) unless turned off; `mise.lock` pins checksums; `mise install --locked`.
- omp's bash sets `CI=true`, which makes mise trust any config; `paranoid = ["config"]` stops that (0473).
- Jarvis's model endpoint loads omp's source from its Bun install (`llm-endpoint.ts:215-253`), so a mise-installed omp would break it until 0463 lands; `omp update` on a mise-installed omp runs `mise upgrade … --bump` (`update-cli.ts`).
- mise installs through `irm https://mise.run/powershell | iex` (2026.10.18) or `winget install jdx.mise` (2026.10.12); shims go in `%LOCALAPPDATA%\mise\shims` on the user Path.

## Options
| Option | Pros | Cons |
|---|---|---|
| Repo + machine tools; omp and Bun as today | One reviewed pin file with checksums; `mise run check`; no break to the model endpoint | A new tool and shims on PATH |
| Everything in mise now, with omp and Bun | One file for all | Breaks the model endpoint until 0463; two Buns |
| Jarvis repo only | Smallest | Machine tools stay by hand |
| No mise | Nothing new | Everything stays manual |

## Recommendation
Repo + machine tools; omp and Bun as today.

## Choice
- The Jarvis repo gets a `mise.toml` pinning `bun = "1.4.3"` and `node = "24.21.0"` with tasks `check` (typecheck, lint, app tsc, tests), `build`, `evals` and `smoke`, and a committed `mise.lock`. `scripts/build.ts` keeps its hash-checked Node and pnpm for what ships.
- `jarvis setup` writes `%USERPROFILE%\.config\mise\conf.d\jarvis.toml` (diff and `y`, 0466) when mise is on PATH: exact pins for `herdr`, `hunk` and `gh` (never `latest` or auto-update; a bump is an edit plus `mise lock --global`, re-checked per 0383) and the settings `use_versions_host = false`, `use_versions_host_track = false`, `auto_install = false`, `paranoid = ["config"]`, `upgrade.auto_prune = false`, plus `min_version`.
- omp stays his Bun global install and Bun stays on `bun upgrade` until 0463 lands and a mise-installed omp is checked; then omp's move is asked again.
- Jarvis ships a content-only mise skill for agents (read `mise tasks ls --json`; prefer `mise run <task>`; never `mise trust`, `mise use`, `upgrade --bump` or `--yes`; never edit `~/.config/mise`; never `omp update` on a mise-installed omp), backed by guard rules (0450) that deny `mise trust`, `mise use -g` and `mise upgrade … --bump` in panes.
- Public users: `install.ps1` doesn't need mise; setup writes the mise file only when mise is installed.
- Install approved: mise through the official script (`irm https://mise.run/powershell | iex`), its shims folder on his user Path.

## Reason
"we can use the recommended" (with his note that Jarvis and its panes use mise to speed up work on projects, 0477) and "The official script (irm https://mise.run/powershell | iex)".

## Consequences
- Amends 0450 (guard rules for mise), 0466 (setup writes the mise file), 0473 (the paranoid setting comes from here).
- The install approval joins plan §4's approved installs (0385, 0435).
