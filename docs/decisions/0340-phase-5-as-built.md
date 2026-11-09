# 0340 Phase 5 as-built, memory and graph

- Status: decided
- Kind: product
- Phase/slice: 5/5.1–5.9
- Date: 2026-10-08

## Question
Are the memory and graph choices the phase 5 build made accepted?

## Facts
- No record set these; each was explained at his request, with how much it matters.
- v1 has no thread close step (0241).

## Options
| Option | Pros | Cons |
|---|---|---|
| Accept as built | Done | — |
| Rework the unticked ones | — | — |

## Recommendation
Accept as built.

## Choice
- Promotion runs when a workspace thread reports back (`report`), since v1 has no close step.
- catalog.db v4: `memory_proposals` (pending, accepted and rejected kept as history) and `memory_imports`; project.db v4: `decisions`.
- The predicted answer is a word-overlap match of at least 0.5 with an earlier question, with no model call; the preview reads `you'd probably say <answer> (like <date> in <thread>: "<earlier question>")`, and the boss's relay adds `From his decision log: <preview>`.
- The prompt carries a `<memory>` section: `# Memory`, a note to propose changes with `remember`, then `## Project <name>` and `## Personal` lists, loaded at session start.
- The importer skips sessions with fewer than 3 of his messages, caps each at 12,000 characters, makes one model call per ~60,000 characters per project, shows counts and a token estimate, asks `Continue? [y/N]`, and skips files it already imported.
- The proposal card reads `memory · <project> · 1 of N`, shows `replaces:` or `remove:` lines struck through, and has accept, edit and reject (Enter accepts an edit, Esc leaves it); a blue border; pending proposals are re-read every 3 seconds.
- The nightly run stamps `<data>/nightly.json`; the first start runs it once; a failure shows `Nightly memory run failed: <reason>` and retries at the next start.
- Containers: Compose project `jarvis` in `~/.jarvis/agent/compose.yaml` (written by Jarvis); FalkorDB 6.0.2 with no published port, its web UI off, volume `falkordb-data`; the Graphiti image (2.2 GB, CPU-only PyTorch, MiniLM embeddings baked in, offline).
- Containers start on first use and stop after 15 minutes idle and at process exit; Graphiti listens on 127.0.0.1:47380.
- `graph_search(query, limit?)` in project threads returns `- <fact> (from … until …)` lines, 10 by default.

## Reason
"more explanation why and which are really useful"; then all ticked.

## Consequences
- Appendix I: catalog.db v4, project.db v4, `<data>/nightly.json`, `~/.jarvis/agent/compose.yaml`, the `graph_search` and `remember` tools, `jarvis memory import`.
- The migration ledger shifts phase 8's event queue to catalog.db v5.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
