# 0562 The code index is measured on each project's main checkout, for relationship questions only

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V11
- Date: 2026-11-09

## Question
0542's spike of codebase-memory-mcp: does it save tokens, how good are its answers, what does it cost, and how is it measured given that every thread works in its own worktree?

## Facts
Observed 2026-11-09 with codebase-memory-mcp 0.12.1 (through mise, scoped to `D:/jocasta-spikes/code-index/`) on two copied fixtures (42 MB and 91 MB of TypeScript), a hand-written MCP client, 8 questions against a grep-and-read baseline; no model calls.
- Bytes: with its full output 39% more than grep over the 8 questions; capped as `graph_search` would pass it on (10 locations, no source) 39% fewer. It saves on relationship questions (callers, impact, implementers: 22–53% raw, up to 93% capped) and costs more on "where is X" and "how does X work".
- Quality: 5 correct, 2 partial, 1 wrong (no callers found through a renamed import, `import { a as b }`).
- Cost: 18–34 s to index, an index about 6× the source (260 MB and 581 MB), 0.6–1.3 GB while indexing, ~35 MB serving, one shared background process that exits with its last client; 17 raw tools would add ~5.2k tokens of schema per session.
- Freshness: only the project the server was started in is watched (an edit showed after 31.6 s); others stay stale until re-indexed (7–18 s incremental). Each worktree is a separate full index.
- Its cache and runtime folders refuse any path whose ancestors grant "Authenticated Users" write, as `D:\` does by default; paths under `C:\Users\<him>\` pass. Repositories on `D:` index fine.
- Only its `install` command writes agent configs (`~/.omp`, `~/.claude`, VS Code); the stdio server doesn't.

## Options
| Option | Pros | Cons |
|---|---|---|
| Measure it, main checkout only | One index per project; relationship questions are mostly about code that already exists | A thread's own edits aren't indexed until it merges |
| Measure it, every worktree | Fresh for each thread | ~6× the source in disk and 20–30 s per thread |
| Drop it now | No build work, no 334 MB tool | 0542's measurement isn't made |

## Recommendation
Measure it, main checkout only.

## Choice
- One index per project's main checkout, refreshed when a thread merges; its data under his user folder on `C:` (inside Jocasta's data dir there).
- `graph_search` uses it only for relationship questions (callers, impact, implementers) and returns at most 10 locations without source; its own tools are never exposed to panes.
- Jocasta starts only its stdio server, never `install` or `update`.
- A6 measures it with the rest of the layer; it's kept only if the benchmark shows it cheaper per passing task (0543's rule).

## Reason
"Measure it, main checkout only".

## Consequences
- Carries out 0542.
- Install approved for the build and the VM: codebase-memory-mcp through mise, pinned (0542).
- Left on disk: `D:/jocasta-spikes/code-index/` (188 MB of fixtures and scripts, one local worktree registration inside it). The mise install and its download cache were removed; the 1.1 GB index cache in his Temp folder was deleted; no agent config touched, nothing running.
