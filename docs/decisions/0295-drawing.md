# 0295 How agents draw

- Status: decided
- Kind: product
- Phase/slice: 7/7.1–7.2
- Date: 2026-10-08

## Question
How do agents draw on Frontier's canvas?

## Facts
Read 2026-10-08 in devdotfast/whiteboard @29a00b6 (MIT):
- Agents write typed JSON blocks: `markdown`, `code_peek` (file and line range), `sequence` (actors and steps, each step linked to code), `flow_diagram` (nodes and edges), `database_lens`, `call_stack_diff`, `software_map` (`packages/review/app/src/block-document-derivations.ts:15-25`; fixture `saved-reviews/three-minute-tour.json`).
- The canvas lays them out with `@xyflow/react` 12.10.2 and `elkjs` 0.11.1 on React 19 (`packages/review/app/package.json`). Clicking a step opens its code. Agents patch nodes or steps by id, and edits appear live. A scratchpad keeps drawings newest first (`packages/review/instructions/scratchpad.md`).
- Its README now offers a Windows download, so plan 7.1's "no Windows build" is stale. Its CLI and MCP server need Node 24; telemetry is on by default (`docs/telemetry.md`).
- Mermaid 12.1.0 is MIT (npm, 2026-10-08).
- His answers: "the recommended but i would love to know how we could use whiteboard for this ( the second suggestion)"; after an explanation of using the app itself: "okay then our own approach but use the whiteboard approach we can use a lot".

## Options
| Option | Pros | Cons |
|---|---|---|
| Whiteboard's block format, rendered in Frontier | Code links, patchable, his favourite reference | A renderer to build |
| The Whiteboard app through its MCP | No renderer | A second app, telemetry, drawings outside Jarvis |
| A Mermaid or SVG `draw` tool | Smallest | No code links |

## Recommendation
Whiteboard's block format, rendered in Frontier.

## Choice
Jarvis's own canvas, taking as much as possible from whiteboard's approach (reference only, nothing copied):
- A `draw` tool takes typed blocks: `markdown`, `code_peek`, `sequence` and `flow_diagram`, with a `source` (`path#Lx-Ly`) on any step or node.
- Clicking a step or node opens its code.
- Agents patch blocks, steps and nodes by id, and edits appear live (the draw-in of 0292).
- Each project keeps a scratchpad of drawings, newest first, saved with the project.
- The renderer libraries are chosen at build time.
- The Whiteboard app itself is not used.

## Reason
"okay then our own approach but use the whiteboard approach we can use a lot"

## Consequences
- Settles 7.1 and 7.2; plan 7.1's Whiteboard option is closed.
- `database_lens`, `call_stack_diff` and `software_map` are after v1, with the pruning pass.
- Amended by 0393: whiteboard's canvas, built from source, replaces Jarvis's canvas and `draw`.
