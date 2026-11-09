# 0296 A speak tool

- Status: decided
- Kind: product
- Phase/slice: 7/7.4
- Date: 2026-10-08

## Question
Can the boss choose to say something aloud?

## Facts
- omp's `tts` tool was never called (appendix A). 0294 limits speech to reports and answers to dictated questions. Proactive talk is 8.7's subject.
- He asked what a speak tool would bring; it was explained.

## Options
| Option | Pros | Cons |
|---|---|---|
| Automatic speech only | Predictable | Less film-like |
| A `speak` tool for the boss | The boss can say one chosen sentence | Noisier; a little cost per call |

## Recommendation
Automatic only.

## Choice
The boss has a `speak` tool for one chosen sentence, under 0294's manner rules (about 30 words, no code, paths or tables) and the HUD mute.

## Reason
"what would speak tool bring"

## Consequences
- Differs from the recommendation.
- Since 0373 his replies are read aloud anyway; `speak` serves the turns he didn't start.
- Amended by 0552: spoken replies go, so `speak` leaves the boss's tools.
