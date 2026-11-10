# 0473 mise in project threads

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
When a project uses mise, how do its threads get the project's tools, env and checks without Jarvis trusting a repo on his behalf?

## Facts
Verified 2026-10-23.
- `mise run`, `mise exec` and `mise install` trust a project's config by themselves; a worktree's config is trusted when the same path in the main checkout is trusted (https://mise.jdx.dev/security.html, "Implicit trust", "Git worktrees").
- `mise env --json` prints the project's env (https://mise.jdx.dev/cli/env.html).
- omp has no mise loader, and an omp extension can't add env to the agent's bash: `shellEnv` reaches only the user's own shell, and the `bash` tool has no `env` field (`local://omarchy-MiseResearch.md`).
- Jarvis already passes per-pane env through herdr's `layout.apply` (`src/frontier/server/herdr.ts:524-527`); herdr applies it to the launched process only.
- omp's bash tool sets `CI=true` on every command (`exec/non-interactive-env.ts:31`, `bash-executor.ts:585`), and mise treats CI as "trust every config and answer yes"; only `paranoid = ["config"]` turns that off (https://mise.jdx.dev/security.html, "CI and --yes"; `local://mise-single-entry.md`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Merge `mise env --json` into the pane env and use `mise run verify`, only for trusted configs | Threads get the project's tools and checks; trust respected; no omp code | Env fixed at pane start until respawn |
| Wrap omp in `mise exec` | Simple | Trusts the config by itself and auto-installs tools |
| No mise support | Nothing to build | mise projects lose their tools and checks in threads |

## Recommendation
Merge `mise env --json` into the pane env and use `mise run verify`, only for trusted configs.

## Choice
When mise is installed with paranoid config trust on (Jarvis's mise file sets `paranoid = ["config"]`, 0476; without it this record does nothing) and a thread's project has a mise config that `mise trust --show` reports trusted, Jarvis merges `mise env --json` (run in the thread folder) into the pane env at start and on respawn; the review step runs `mise run verify` (else `mise run test`) as the project's own check (0453) when the project defines it and is trusted. Jarvis never runs `mise trust` on a repo's own config. Jarvis's own per-project mise configs (0477) live in mise's global folder and are trusted by path. Without mise, nothing changes.

## Reason
"Threads get the project's mise environment variables" and "The review runs the project's `mise run verify` (or `test`) when it has one".

## Consequences
- Amends 0453 (a trusted mise `verify`/`test` task is the project's own check) and 0457 (the pane env also carries the trusted project's mise env).
- Whether Windows mise's PATH works in omp's Git Bash is checked by the builder.
- Installing mise on his machine stays his decision.
- Amended by 0476 and 0477: the paranoid setting comes from Jarvis's mise file; Jarvis's own per-project configs need no repo trust.
- Amended by 0596: under paranoid trust, Jocasta trusts a thread worktree's unchanged copy of a config the main checkout has trusted; the worktree fact above holds only without paranoid trust.
