# 0292 Animations

- Status: decided
- Kind: product
- Phase/slice: 7/7.3
- Date: 2026-10-08

## Question
What animates in Frontier?

## Facts
Read 2026-10-08 in devdotfast/whiteboard @29a00b6 (MIT): a draw queue builds a diagram step by step as the agent's edits arrive (`packages/review/app/src/draw-queue.ts`), and full-screen tours step through a sequence, flow or database diagram one stop at a time (`review-panel-store.ts:71`).
- Frontier's look is clean and minimal (0258).

## Options
| Option | Pros | Cons |
|---|---|---|
| Draw-in, tours and HUD pulses | Stolen from whiteboard; fits 0258 | — |
| A film-style HUD (rings, glow) | The film look | Conflicts with 0258 |
| Draw-in only | Smallest | — |

## Recommendation
Draw-in, tours and HUD pulses.

## Choice
Diagrams build in step by step as the agent draws them; a sequence or flow diagram can be toured full screen one step at a time with its code; the HUD's mic and the thread dots pulse while listening or thinking. Nothing decorative beyond that.

## Reason
Not given.

## Consequences
- Slice 7.3 is decided.
- Refined by 0371: strokes traced by a point of light, labels wiped in, fresh ink drying; the tour lists its stops.
