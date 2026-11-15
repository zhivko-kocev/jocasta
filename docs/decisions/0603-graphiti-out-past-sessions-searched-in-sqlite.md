# 0603 Graphiti and FalkorDB go; past sessions are searched in SQLite

- Status: v1
- Kind: product, benchmark
- Phase/slice: program
- Date: 2026-11-14

## Question
Does Jocasta keep Graphiti as its memory of past sessions?

## Facts
Measured 2026-11-14 with the containers started for the test and stopped after it (`docs/research/state-of-the-art.md` section 4).
- Graphiti runs: healthy in 23 s, 473 MiB idle and 702 MiB after 30 searches, images 2.9 GB. The "not working since 11-12" in the cost report was Docker restarting the container at boot without FalkorDB ([INFERENCE] from exit codes and restart counts).
- The graph never had a full-text index: FalkorDB rejects the driver's `createNodeIndex` call on every search, so search has run on embeddings alone since it shipped.
- Content: 14 episodes (12 handoff summaries, 2 decision notes), 162 facts, 71 of them already ended. Of the identifiers in the episodes the graph still holds 31% of the record ids, 26% of the file paths and 21% of the commit hashes.
- 30 questions on the same text: the graph's answer names the right record in 8, SQLite FTS5 (BM25) over the same summaries' lines in 14; judged on 10 questions, 3 against 3–4; 108 ms against 1 ms a query. 38% of the facts the graph returned were ended ones.
- Cost: $0.42 an episode on the large model, written to `graph-usage.jsonl` as `cost: 0`.
- Use: one `graph_search` call since the cutover; in the same window agents read `docs/decisions/`, the plan, the status and the changelog in 550 of 2,979 tool calls.
- `catalog.db`'s `summaries` already keeps every summary the watcher reads (0389), 108 rows; Graphiti got 14.
- No published coding evaluation of graph memory was found (21 sources, section 4).
- V9 asks for "persistent memory, a knowledge graph or SQLite".

## Options
| Option | Pros | Cons |
|---|---|---|
| Remove Graphiti and FalkorDB; the search tool answers from SQLite text search over the stored summaries | No model call, no Docker, no ingest cost; did at least as well in the test | Exact words only; a fact's "ended" date is gone |
| Remove Graphiti and the search tool too | Most code deleted | A project without written records has nothing to search |
| Keep Graphiti and fix the index, the restart policy and the cost log | Facts with dates that tolerate paraphrase | Docker and 700 MiB stay; nothing measured shows it helps |
| Freeze: stop ingesting, keep the code | Reversible | Dead code stays |

## Recommendation
Remove Graphiti and FalkorDB and answer from SQLite: one memory, the summaries already kept, searched by code.

## Choice
Graphiti and FalkorDB are removed: the image (`docker/graphiti/`), the service's model endpoint and proxy for it (0390, 0463) with its cost line, and the watcher's posting. Graphiti was the container manager's only resource, so the manager goes too and Jocasta no longer uses Docker (0072, 0074).

The watcher keeps saving each compaction and handoff summary to `catalog.db` (0389). The search tool answers from a SQLite FTS5 index (BM25) over the lines of the summaries 0400 sent to Graphiti (handoff and soft compactions of sessions in a project) and of `report_result`'s decisions (0539). It stays sealed to the pane's project, with the boss naming the project (0484), and gives each line with its source session and date, quoted as data. No model call and no container are involved. The index is derived data: it can be rebuilt from `catalog.db` and the task files.

## Reason
His answer: "fts".

## Consequences
- Amends 0334, 0389, 0390, 0400, 0463, 0539 and 0591's graph line; 0072 and 0074 have no resource left.
- `catalog.db` gets a migration for the index (plan appendix I).
- On his PC, setup takes the two containers down and removes the compose file and the Graphiti token; the images and the volume with the old graph go only on his yes, the way uninstall asks today.
- H4 plants its canaries through `report_result` and queries the same tool; it no longer waits for an extraction.
- "$ today" loses its graph line; the ledger no longer has a spend it cannot see.
- The tool's name is 0604; the code index behind it is 0605.
