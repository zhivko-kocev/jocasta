# 0278 Memory layers

- Status: decided
- Kind: product
- Phase/slice: 5/5.0–5.5
- Date: 2026-10-08

## Question
How is memory built?

## Facts
- His proposal (free text, quoted in Reason) lays out five layers: a curated `memory.md`, SQLite for what happened, Graphiti for long-term knowledge and relations, git for code history, a code index for the current code, plus a promotion pipeline (observation → evaluator → discard, graph or `memory.md`).
- Already in Jarvis: Pi's session files hold every message and tool call; `project.db` holds threads, states and reports (0096, 0247); `history_search` reads them (0245). Code search is Pi's `grep` and `find`, with LSP (6.6) and ast-grep (6.9) in phase 6.
- Claude Code's auto memory loads a `MEMORY.md` index at start (first 200 lines or 25 KB) and reads topic files on demand (https://code.claude.com/docs/en/memory, 2026-10-08).
- Verified 2026-10-08 (https://github.com/getzep/graphiti): Graphiti needs Python 3.10+, a graph database (Neo4j, FalkorDB, Neptune or Kuzu) and a model for extraction. Headless resources run in containers (0070); Docker Desktop is installed (0071).
- Committing `memory.md` into his repos, client repos included, conflicts with his rules (0067).
- His earlier answers: "why to drop c?" and "why not combine all three?".

## Options
| Option | Pros | Cons |
|---|---|---|
| Adopt the layers on what exists; `memory.md` per project in the data dir; Graphiti kept only if a spike shows a gain | His design; little new code | A spike to run |
| Adopt with Graphiti in v1, no spike | Everything built | A Python service and per-episode model calls, unmeasured |
| Adopt with Graphiti after v1 | Smallest | No relation queries in v1 |

## Recommendation
Adopt, Graphiti by spike.

## Choice
Now (0334): Graphiti is in v1 with no spike first, routed through Jarvis; the 30-question comparison runs after the build as an eval.

Originally:
- `memory.md`: one per project at `<data>/projects/<pid>/memory.md`, loaded at thread start (at most 200 lines); not in the repo, and copied into the repo's `AGENTS.md` only on his word.
- Personal store (0277): `~/.jarvis/agent/memory.md`, loaded in every project.
- What happened: Pi's sessions and `project.db`, read through `history_search` (0245); no second SQLite memory store.
- Git and the code index: referenced, not duplicated (phase 6 for LSP and ast-grep).
- Graphiti: in a container (0070), kept in v1 only if a spike shows at least 5 more correct answers out of 30 "why" and "what does this affect" questions than `memory.md` plus `history_search`, with its cost per day measured; otherwise after v1. Installing it asks him first. (Amended by 0334.)
- Promotion: when a workspace thread closes, one evaluator call sorts what it learned into discard, the graph (if kept), or a `memory.md` proposal.

## Reason
His proposal, in full in the transcript, beginning "The coding harness should use multiple complementary memory systems rather than relying on a single long-term memory store."; and on the recommendation: "i love the recommended".

## Consequences
- Replaces plan 5.0's three-way spike and 5.1's `me.db` and `group.db` design.
- The boss's memory is 0280.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
