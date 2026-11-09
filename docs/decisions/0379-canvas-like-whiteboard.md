# 0379 The canvas like whiteboard

- Status: decided
- Kind: product
- Phase/slice: 7/7.2–7.3
- Date: 2026-10-10

## Question
How does the canvas stop parts overlapping, and how does it move?

## Facts
Verified 2026-10-10.
- Every flow node is a fixed 180×48 box; ELK gets no edge labels; libavoid re-routes edges orthogonally. Edge labels are drawn at the route's middle point with no halo; on a two-point route that is the end, so the label sits on the arrowhead.
- Sequences: actors 120×34 with a 44 gap, rows 46, self-loops 46.
- libavoid's wasm is why the CSP carries `'unsafe-eval'`.
- Motion: a part starts every 220 ms and each motion lasts 640 ms (0371).
- Whiteboard: flows in fixed 210×62 boxes with the name cut to one line at 26 characters, ELK's own routes, edge labels known to ELK (TAIL) with a halo; sequences in equal lanes of at least 176 px, wrapped labels, 76 px rows; motion: outline 420 ms, fill 330 ms while the label types in, the edge 450 ms then its arrowhead, and a quick pass for whole drawings.
- Measured 2026-10-12 with 210×62 boxes: TAIL labels 20 px tall overlapped each other on a three-way fan-out drawn down and on a two-way branch drawn right; at whiteboard's 12 px (9px text) only a label crossing a sibling's line remained, on fan-outs drawn down; CENTER placement had no overlaps. The 9px label font's mean advance in headless Edge is 5.173 px (whiteboard uses 5.4).

## Options
| Option | Pros | Cons |
|---|---|---|
| Whiteboard's sizes, ELK's own routes and labels, its motion | No overlaps by construction; one layout engine; drops a wasm and `'unsafe-eval'` | Long names are cut to one line |
| Keep libavoid and tune spacing | Fewer changes | Labels still unknown to the layout |

## Recommendation
Whiteboard's way.

## Choice
- Flows: fixed 210×62 boxes; names over 26 characters cut to 25 plus "…", the full name in `aria-label`, `title` and the tour; edge labels over 28 characters cut to 27 plus "…", given to ELK with TAIL placement at whiteboard's size (9px text, 12 px tall, characters × the measured advance) and drawn with a halo; a label may cross a sibling's line on a fan-out; ELK layered with orthogonal routing, node spacing 36, 56 between layers, model order kept; routes are ELK's sections. libavoid is removed.
- Sequences: equal 176 px lanes and 76 px rows; one-line actor chips; message labels wrap to at most 2 lines on a surface-coloured box; a self-loop draws right of its lifeline, left in the last lane, with its step number on the other side.
- Fit uses the layout's whole bounds with zoom never above 1; 0374's frame rule stays.
- Motion: parts play one at a time; a node's outline 420 ms, then a 330 ms fill while its label types in; an edge strokes 450 ms, then its arrowhead appears. A drawing that arrives whole plays a quick pass, 220 ms per part for at most 16 parts, then the rest at once. Reduced motion shows everything at once.

## Reason
His direction: the canvas moves like whiteboard's with no overlaps, built now: fixed boxes with a one-line cut, and ELK's own routes. On label placement, his answer (2026-10-12): "Whiteboard's way: TAIL + 12px labels".

## Consequences
- Amends 0352 (ELK without libavoid; `@mr_mint/elkjs-libavoid` is removed) and 0371 (timings).
- The CSP becomes `script-src 'self' 'wasm-unsafe-eval'`, kept for the voice's wasm.
- Amended by 0393: whiteboard's canvas, built from source, replaces Jarvis's canvas and `draw`.
