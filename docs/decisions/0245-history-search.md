# 0245 History search

- Status: decided
- Kind: product
- Phase/slice: 3/3.5
- Date: 2026-10-08

## Question
How are earlier windows searched?

## Facts
- Pi's compaction never deletes entries; a thread's session files live under `<data>/projects/<pid>/threads/<tid>/` (0131).
- If billion-context wins 3.4, its `search_context` and `decompress` cover this (0216).

## Options
| Option | Pros | Cons |
|---|---|---|
| One `history_search` tool over the thread's session files | One tool | No URL scheme |
| The tool plus `history://` reads | Readable by URL | A scheme to build |
| Readable only | Nothing to build | The model can't search |

## Recommendation
The tool.

## Choice
Now (0323): not built: billion-context's `search_context` covers history search.

Originally: One `history_search` tool: a plain-text match over the thread's session files, returning snippets with their window and entry ids. Not built if billion-context wins 3.4.

## Reason
Not given.

## Consequences
- Phase 3's exit smoke reads: `history_search` finds a line from the first window.
- Amended by 0323: not built.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
