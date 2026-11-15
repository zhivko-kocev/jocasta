# 0605 The code index is removed

- Status: v1
- Kind: product, benchmark
- Phase/slice: program
- Date: 2026-11-14

## Question
Does Jocasta keep the code index (codebase-memory-mcp) behind the search tool?

## Facts
Measured 2026-11-14 (`docs/research/state-of-the-art.md` section 4).
- The index was never built under Jocasta on his PC: no index folder and no call since the cutover. Its binary is 318 MB and its manager `src/frontier/server/code-index.ts` is 604 lines; mise pins it (`src/setup/mise.ts`).
- 0542 and 0562 keep it only if the benchmark shows it cheaper per passing task (0543's rule); that benchmark has not run on this build.
- A controlled study gave Claude Code with Sonnet 4.6 a code-graph MCP tool over 300 tasks: no Pass@1 gain, cost up 8–28% in four of eight settings (section 4).
- omp's `lsp` tool already answers callers, references and implementers in the panes.

## Options
| Option | Pros | Cons |
|---|---|---|
| Remove it and its mise pin; relationship questions go to omp's `lsp` | 604 lines and a 318 MB binary gone | The follow-up arm `f-index-off` loses its subject |
| Leave it dormant until the benchmark arm runs | Follows 0543's keep rule | Unused code and binary stay |

## Recommendation
Remove it.

## Choice
The code index is removed: its manager, its part of the search tool's answer, its mise pin and its place in A6's layer. Relationship questions go to omp's `lsp`.

## Reason
His answer: "remove".

## Consequences
- Amends 0542 (K5), 0562 and 0543's list of follow-up arms; 0575's index build before the trial's clock has nothing left to build.
- The follow-up study drops `f-index-off`, as 0571 dropped `f-check-off`: A6 without the index is A6.
- A6's layer still lists `history_search` (0604), as the product's thread panes do (0543); in a trial it has no past sessions to answer from.
- Setup stops pinning codebase-memory-mcp; a copy mise already installed is his to remove with `mise uninstall`.
