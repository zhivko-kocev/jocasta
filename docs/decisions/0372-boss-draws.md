# 0372 The boss draws

- Status: decided
- Kind: product
- Phase/slice: 7/7.2, 4/4.4
- Vision: V6, V11
- Date: 2026-10-10

## Question
Should the boss draw on Frontier's canvas?

## Facts
- 0354 accepted "the boss doesn't draw": `draw` is left out of the boss's tools (`BOSS_TOOLS` in `src/frontier/boss/tools.ts`), though the boss's session loads the extension and the server already sends the boss's own drawings to the canvas.
- In his beta on 2026-10-10 he asked the boss to draw; it answered: "None of the tools I have here can draw on the canvas or reach it at all".
- The boss has no file tools (0261), so it can't check a `source`'s lines itself.

## Options
| Option | Pros | Cons |
|---|---|---|
| The boss gets `draw`, on his own scratchpad or a project's; his manual says when to draw | Explanations, plans and overviews come with a diagram straight from the chat | One more boss tool |
| The boss asks a thread to draw | 0354 unchanged | Slower: a thread has to start first |

## Recommendation
The boss draws.

## Choice
- The boss has `draw` with the threads' blocks and edits, plus an optional `project`: without it the drawing goes on Jarvis's own scratchpad (`<data>/home/drawings/`), with it on that project's scratchpad, where a `source` resolves in the project's checkout.
- His manual: draw when a picture explains better than words (a plan, an overview of the threads' work, how the parts of a change fit together); start with a title and build the drawing up in a few calls; say in the reply that it is on the canvas. With no file tools, he gives a `source` only with lines a thread reported.
- The canvas credits the boss's drawings to Jarvis: "Drawn by Jarvis".

## Reason
"Boss draws (Recommended)".

## Consequences
- Amends 0354 ("the boss doesn't draw") and adds a tool to 0261's.
- Plan 4.4, 7.2 and appendix I carry it.
- The byline and the manual's wording were chosen by the builders under 0363.
- Amended by 0393: whiteboard's canvas, built from source, replaces Jarvis's canvas and `draw`.
