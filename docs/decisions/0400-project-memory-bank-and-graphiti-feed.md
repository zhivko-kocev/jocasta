# 0400 One memory bank per project, and what feeds Graphiti

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
How do mnemopi and Graphiti work per project, and what feeds Graphiti?

## Facts
Verified 2026-10-13.
- mnemopi is omp's local SQLite memory backend (`~/.omp/agent/memories/mnemopi/mnemopi.db`); with `mnemopi.llmMode: smol` the `memory` model role extracts facts to retain; recall injects relevant memories each turn.
- mnemopi's default `per-project` scoping hashes the absolute cwd (`mnemopi/config.ts:199-215`), so each worktree gets its own bank; `mnemopi.scoping: global` with `mnemopi.bank: <name>` uses one named bank.
- omp loads `<repo>/.omp/AGENTS.md` (nearest non-empty `.omp/`) into every session with no code.
- Graphiti is written only by the companion's watcher (0389); agents read it with `graph_search`.

## Options
| Option | Pros | Cons |
|---|---|---|
| One bank per project via Jarvis's overlay | All of a project's worktrees share one bank; nothing written into his repos | Plain omp opened by hand in the repo uses its per-cwd bank |
| Per project via `<repo>/.omp/config.yml` | Plain omp agrees too | Files in his repos and every worktree |
| omp's default per cwd | No change | Memory fragments per worktree |

## Recommendation
One bank per project via Jarvis's overlay; Graphiti fed by summaries plus a task-end `/handoff`.

## Choice
`memory.backend: mnemopi` with `mnemopi.llmMode: smol`. Jarvis writes `<data>/projects/<pid>/omp.yml` = `{mnemopi: {scoping: global, bank: <project id>}, plan: {defaultOnStartup: true}}` and passes it as `--config` to every project and thread pane, so a project's worktrees share one bank. The boss pane in `<data>/home` keeps its own bank and never recalls project banks. Hand-kept project notes live in `<repo>/.omp/AGENTS.md`; the old `memory.md` files stay on disk. Graphiti gets every handoff or soft summary from watched projects' sessions, and when a thread pane goes from working to idle with new commits on its branch, the companion sends `/handoff` to it so each finished task leaves one summary; the episode goes into the project's group (same project id as the bank).

## Reason
"mnemopi + Graphiti; curated notes in .omp/AGENTS.md"; "smol (memory role)"; "recommended and what about graphiti"; "Yes, isolated boss bank"; "Summaries + /handoff at task end".

## Consequences
- Amends 0389: memory.md's role goes to `.omp/AGENTS.md`; the task-end `/handoff` is new.
- The task-end trigger is the same working → idle event as F4's review (0394); the companion runs the review first, then the handoff.
- Appendix I gains `<data>/projects/<pid>/omp.yml`.
- Amended by 0440: a merged thread's handoff keeps its project (`summaries.project_id`) and still reaches Graphiti after a companion restart.
- Amended by 0447: the overlay also sets `memory.backend: mnemopi`, `mnemopi.llmMode: smol` and a per-project `mnemopi.dbPath`; the boss uses `<data>/home/mnemopi.db`.
- Amended by 0449: the overlay sets `contextFiles.extra: [AGENTS.local.md]` for private notes that reach every thread.
