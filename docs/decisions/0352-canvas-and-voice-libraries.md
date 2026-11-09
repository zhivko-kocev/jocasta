# 0352 The canvas and voice libraries

- Status: decided
- Kind: product
- Phase/slice: 7/7.2–7.4
- Date: 2026-10-09

## Question
Which libraries draw the canvas and speak, now that 0293 and 0295 left them to the build?

## Facts
Verified 2026-10-09 on npm and in devdotfast/whiteboard @25277f4 (MIT, read-only, nothing copied).
- Whiteboard draws sequence and flow diagrams on React Flow (`@xyflow/react`): steps and participants are React nodes, messages are custom edges. It lays out flow diagrams with ELK (`elkjs`, loaded lazily because it is about half the canvas bundle) and routes edges with `elkjs-libavoid`. It highlights code with `@speed-highlight/core` and animates the draw-in with a queue of motion phases.
- Versions: `@xyflow/react` 12.12.0 (MIT); `elkjs` 0.12.0 (EPL-2.0 or GPL-3.0-or-later); `@mr_mint/elkjs-libavoid` 0.5.0 (MIT on npm); `marked` 18.1.0 (MIT); `shiki` 4.5.0 (MIT); `kokoro-js` 1.2.1 (Apache-2.0, last published 2025-05, still the latest; its model is about 90 MB, downloaded on first use).
- Jarvis's page already runs React 19 (0328).
- He asked what each layout option gains and loses; both were explained.

## Options
| Option | Pros | Cons |
|---|---|---|
| React Flow with ELK and libavoid | Whiteboard-quality layouts, right-angle edges around boxes, nested groups | Larger bundle, loaded lazily; ELK's EPL-2.0 in a public repo |
| React Flow with dagre | Small, MIT, fast | Plainer routing; no groups or ports |
| Own SVG with dagre | No canvas library | All drawing and hit-testing ours |

Code and markdown: `marked` with `shiki` | `marked` with `highlight.js` | plain text.

## Recommendation
React Flow with dagre; `marked` with `shiki`.

## Choice
- The canvas uses React Flow for both diagram kinds, ELK for flow layout (loaded on first use), and its own layout for sequence diagrams. Now (0379): ELK routes flows and places their edge labels; libavoid is removed. Originally: ELK with libavoid.
- `markdown` and `code_peek` blocks use `marked` and `shiki`.
- `kokoro-js` speaks; its model downloads once on first use and is cached (0293).
- Installs approved: all six packages, pinned exactly.

## Reason
"i like the recomended option but see how whiteboard handles the drawing"; "what do i get or lose with both approaches with react flow"; then React Flow with ELK and libavoid. Voice: "Approve both".

## Consequences
- README's Credits name whiteboard as the reference for the canvas.
- Differs from the recommendation on layout.
- Amended by 0393: whiteboard's canvas, built from source, replaces Jarvis's canvas and `draw`.
