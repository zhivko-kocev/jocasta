# 0413 Every thread turn end is spoken

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
When a thread goes from working to idle without new commits, is that spoken and toasted like blocked and done?

## Facts
Verified 2026-10-13.
- Built in F2: blocked and done-with-commits threads give a boss `[event]` line, a spoken line and a toast; a plain turn end gave only `[event] … finished its turn: <last line>`.
- omp's herdr integration reports working, blocked and idle only; failed runs can't be told apart.

## Options
| Option | Pros | Cons |
|---|---|---|
| Boss line only | Less noise | He may miss a thread that stopped to ask in prose |
| Speak it too | He hears every turn end | More speech |

## Recommendation
Boss line only.

## Choice
A plain turn end is also spoken (one short line) besides the boss's `[event]` line; toasts stay for urgent and blocked events.

## Reason
"Speak it too".

## Consequences
- Differs from the recommendation.
- Amends 0405.
