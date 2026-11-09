# 0361 `report` can end a thread blocked

- Status: decided
- Kind: product
- Phase/slice: 3/3.7
- Date: 2026-10-09

## Question
0359's guideline says "call report with status failed or blocked", but `report`'s status is done, failed or cancelled (0247). Which gives?

## Facts
- Found while building 0359: a model following the line with `blocked` fails the tool's schema.
- A thread waiting on a question already shows as blocked through `ask` (0246).

## Options
| Option | Pros | Cons |
|---|---|---|
| Change the line to "status failed" | No new state | A thread that can't go on without him reads as failed |
| Add `blocked` to `report`'s status | The thread ends blocked with its reason, and the boss relays it | One more ending state |

## Recommendation
Change the line to "status failed".

## Choice
`report`'s status gains `blocked`: the thread ends in the blocked state, and its reason reaches the boss like other blocked threads. 0359's guideline stays as written.

## Reason
Not given.

## Consequences
- Amends 0247's status list.
- Differs from the recommendation.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
