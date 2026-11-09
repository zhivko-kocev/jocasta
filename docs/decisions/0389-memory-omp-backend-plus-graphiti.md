# 0389 Memory: omp's backend plus Graphiti

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
What is Jarvis's memory when the harness is omp?

## Facts
Verified 2026-10-13.
- omp's memory backends: `hindsight`, `mnemopi` ("local Mnemopi SQLite memory"), `sharpshooter`, `local`, `off` (`src/memory-backend/resolve.ts`).
- Jarvis's Graphiti service (`docker/graphiti/service.py`, graphiti-core 0.30.2, FalkorDB, local MiniLM embeddings) serves `/health`, `/episodes`, `/search`.

## Options
| Option | Pros | Cons |
|---|---|---|
| omp's SQLite backend + Graphiti fed by a session watcher | No Jarvis code in omp; every compaction and handoff summary is kept; the graph only gets watched projects | Graph writes come from summaries only |
| omp's backend only | Nothing to run | No knowledge graph |
| Keep Jarvis's memory.md and proposals | As built | Jarvis code inside omp |

## Recommendation
omp's SQLite backend + Graphiti fed by a session watcher.

## Choice
Memory = omp's memory backend (the SQLite one, chosen in the walkthrough) + Graphiti in the existing containers. Threads and the boss search the graph with `graph_search` from `jarvis mcp`. The companion's session watcher saves every compaction and handoff summary to `catalog.db` and, for sessions in a watched project, as a Graphiti episode in that project's group. Jarvis's `memory.md`, proposals, decision log, nightly consolidation and importer retire.

## Reason
"+ the memmory stuff with wqlite and graphity"; "some tweaks for saving knowledge in the knowledge graph db and other stuff in sqlite".

## Consequences
- `catalog.db` v6 adds `summaries`; `<data>/watcher.json` holds the last scan.
- Sessions outside watched projects never reach Graphiti (principle 6).
- Amended by 0400: hand-kept project notes live in `<repo>/.omp/AGENTS.md`; one mnemopi bank per project through Jarvis's overlay; a task-end `/handoff` feeds Graphiti.
- Amended by 0440: `catalog.db` v7 adds `summaries.project_id`, set when the summary is stored.
