# 0478 The privacy settings ride in every Jarvis pane

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
Three of 0451's settings keep data on the machine: Auto QA off, Claude Code plugins off, web search on DuckDuckGo. They reach his omp only through his global `config.yml` (by hand or setup's diff, 0466). Should every Jarvis pane carry them itself, like memory, compaction and the guard (0447, 0448, 0450)?

## Facts
Verified 2026-10-23.
- His live `config.yml` has `dev.autoqaConsent: granted` and no `dev.autoqa: false`, so tool reports go to `qa.omp.sh` with the install id and free text that can name client paths (omp `tools/settings.ts`, `tools/report-tool-issue.ts`); `modelRoles.web: web/parallel` sends each query with the session id and model name to search.parallel.ai (`web/search/providers/parallel.ts:83-93`); `disabledProviders` is unset, so his Claude Code plugins load into every pane (`discovery/claude-plugins.ts:32,47`).
- omp merges a `--config` overlay over his config key by key for objects and replaces lists (`config/settings.ts:3935-3957`), so a pane's `modelRoles.web` leaves his other roles alone, and its `disabledProviders` replaces his list (he has none).

## Options
| Option | Pros | Cons |
|---|---|---|
| In every Jarvis pane | Jarvis panes stay private whatever his global config says | The same keys live in two places (his config and the pane settings) |
| Only his global config | One place | A Jarvis pane follows whatever he set |

## Recommendation
In every Jarvis pane.

## Choice
Every project and thread pane's settings (`<data>/projects/<pid>/omp.yml`) and the boss's `config/boss.yml` also set `dev.autoqa: false`, `disabledProviders: [claude-plugins]` and `modelRoles.web: web/duckduckgo`. His own omp sessions outside Jarvis follow his `/settings` (0451).

## Reason
"Yes, in every Jarvis pane".

## Consequences
- Amends 0447 (the pane settings gain three keys) and 0451 (these three also ride in the panes).
- Plan appendix I's panes paragraph lists them.
