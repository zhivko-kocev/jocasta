# 0447 Each project's memory in its own file, set by the pane settings

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
0400 gives each project a named mnemopi bank. Where does that bank live, and does a pane's memory depend on his global omp config?

## Facts
Verified 2026-10-23.
- With `mnemopi.scoping: global` and `mnemopi.bank: <pid>`, omp stores every named bank in the one shared `mnemopi.db`: `resolveBankDbPath` returns `config.dbPath` when the bank is the global bank (`omp-src/mnemopi/state.ts:1143-1148`, `config.ts:159-166`); projects are kept apart only by `session_id = <pid>` (`recall.ts:466-473`).
- `/memory clear` deletes that file and its `-wal`/`-shm` (`mnemopi/backend.ts:182`, `733-737`), so a clear in any pane wipes every project's memory; rows with `scope='global'` reach every project (`recall.ts:441,467`).
- An explicit `mnemopi.dbPath` picks the file (`pi-mnemopi/src/core/memory.ts:282-284`).
- His live config has `memory.backend: local` (`~/.omp/agent/config.yml:69-70`); `jarvis setup` writes `config/omp-config.yml` only when `config.yml` is missing (`src/setup/setup.ts:242-243`), so 0400's overlay keys are inert and each worktree gets its own per-cwd memory (omp#2253).
- The pane overlay sets only `mnemopi.scoping`/`bank` and `plan.defaultOnStartup` (`src/frontier/server/service.ts:163-165`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Own file per project in the overlay | Real file isolation; `/memory clear` stays per project; panes work whatever his global config says | Two more overlay keys |
| Keep the shared file | No change | A clear in any pane wipes every project; global rows cross projects |
| Per-project scoping | omp's default | Splits a project's worktrees into separate banks |

## Recommendation
Own file per project in the overlay.

## Choice
The per-project overlay `<data>/projects/<pid>/omp.yml` also sets `memory.backend: mnemopi`, `mnemopi.llmMode: smol` and `mnemopi.dbPath: <data>/projects/<pid>/mnemopi.db` (scoping and bank as in 0400). The boss pane's config (0424, `config/boss.yml`) sets `memory.backend: mnemopi` with `mnemopi.dbPath: <data>/home/mnemopi.db`. Panes no longer depend on his global memory settings.

## Reason
"Own file per project in the overlay".

## Consequences
- Amends 0400 (the overlay's keys; one shared `mnemopi.db`).
- `/memory clear` in a pane clears only that project's (or the boss's) memory.
- Plan appendix I lists the overlay's new keys and the two `mnemopi.db` paths.
- Amended by 0478: the pane settings also set `dev.autoqa: false`, `disabledProviders: [claude-plugins]` and `modelRoles.web: web/duckduckgo`.
