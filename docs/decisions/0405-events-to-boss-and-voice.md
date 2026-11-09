# 0405 Events reach the boss and the voice

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
How do blocked threads and Jarvis's events reach him?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).
- herdr's omp integration reports working, blocked and idle; the service sees every herdr state change, poller event and reminder.

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr only | Nothing to build | The boss doesn't know |
| herdr + boss events | The boss can act on each event | Event lines in the boss pane |
| Events + spoken | He hears it too | Speech for each event |
| Digest only | Quiet | Late |

## Recommendation
herdr + boss events.

## Choice
Each change (blocked, done, failed, review ready, poller and reminder events) becomes one `[event]` line in the boss pane when it is idle (urgent ones at once), and the companion speaks one short line for it ("Export would like a word."), within quiet hours (0298).

## Reason
"Events + spoken".

## Consequences
- Differs from the recommendation.
- Amends 0373: the boss's answers are spoken per completed message (0392), and events get their own short line.
- Amended by 0413: plain turn ends are spoken too.
