# 0258 Frontier look and motion

- Status: decided
- Kind: product
- Phase/slice: 4/4.3
- Date: 2026-10-08

## Question
How does Frontier look and move?

## Facts
- His words: "like jarvis in the movie but with animatios" and "simple but cool and smooth and optimal" (0249).
- Ambient motion keeps the GPU and CPU busy; event-driven CSS motion costs nothing when idle.

## Options
| Option | Pros | Cons |
|---|---|---|
| Movie HUD with event-driven motion | The movie feel; idle CPU near zero | — |
| Movie HUD, always alive | Most cinematic | Constant GPU and CPU use |
| Clean minimal | Calm; smallest | Less of the movie feel |

## Recommendation
Movie HUD with event-driven motion.

## Choice
Clean minimal: a plain dark UI; motion only for state changes.

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- Diagrams still draw on the canvas (0257); their animation is plain state-change motion.
- Amended by 0369: the look is JARVIS, composed; the motion rule stands.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0395: the look is re-decided in the UX pass before F2.
