# 0371 The canvas draws like a hand

- Status: decided
- Kind: product
- Phase/slice: 7/7.3
- Vision: V6
- Date: 2026-10-09

## Question
How do diagrams draw in on Frontier's canvas, now that he asked for whiteboard's animations?

## Facts
Read 2026-10-09 in devdotfast/whiteboard @d97de7d (MIT, read only, nothing copied):
- Its draw queue gives each element motion phases written to `data-motion`: a node's outline is traced with a dash offset (420 ms), then filled (330 ms), then its edge is drawn (450 ms); a label is revealed after its line; easing `cubic-bezier(0.2, 0.7, 0.2, 1)`; reduced motion makes every phase 0 ms.
- Its tour lists every stop with a numbered circle, fills the active one, and jumps straight to a stop so the highlight never flickers through the ones between.
- Frontier's draw-in (0292, `draw-in.ts`) already queues new parts in drawing order and marks them `in` or `changed`; it showed them with a fade and a scale.
- His words: "i would love the whiteboard animations for the canvas".

## Options
| Option | Pros | Cons |
|---|---|---|
| Strokes traced by a point of light, labels wiped in, fresh ink drying | Reads like a hand drawing; still event-driven | Longer motions |
| Keep the fade and scale | Done | Not whiteboard's drawing |

## Recommendation
Strokes traced by a point of light.

## Choice
- New parts still appear one at a time in drawing order (0292's queue).
- A box's outline is traced clockwise from its top-left corner, then it takes a faint wash and its name appears. A lifeline falls from its actor.
- A message or flow edge is traced from its source to its target by a small point of light; its arrowhead lands at the end, and its label is wiped in in the direction it travels.
- Fresh ink is bright and a touch heavier, and dries to its resting colour over about a second. A patched part glows once and settles.
- The tour lists every stop, numbered, beside the diagram; the active stop shows its code; the rest of the diagram dims.
- Now (0379): parts play one at a time: a box's outline in 420 ms, then a 330 ms wash while its name types in; an edge is traced in 450 ms, then its arrowhead appears. A drawing that arrives whole plays a quick pass, 220 ms per part for at most 16 parts, then the rest at once. Reduced motion shows everything at once. Originally: a part starts every 220 ms; each motion lasts 640 ms.

## Reason
"i would love the whiteboard animations for the canvas".

## Consequences
- Refines 0292: the queue and its data are unchanged; only the motion and the tour's side panel change.
- The timings and the tour's list were chosen by the builders under 0363.
- Amended by 0393: whiteboard's canvas, built from source, replaces Jarvis's canvas and `draw`.
- Amended by 0395: the look is re-decided in the UX pass before F2.
