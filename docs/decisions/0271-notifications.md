# 0271 Notifications

- Status: decided
- Kind: product
- Phase/slice: 4/4.9
- Date: 2026-10-08

## Question
How does Frontier signal a blocked or finished thread when it isn't focused?

## Facts
- A web UI shows Windows toasts through the browser Notification API; there is no terminal bell. herdr plays a sound per state (https://github.com/herdrdev/herdr).

## Options
| Option | Pros | Cons |
|---|---|---|
| A toast for blocked and done | Quiet | Easy to miss |
| A toast plus a sound | Hard to miss | Noise |
| None | — | Missed blocks |

## Recommendation
A toast.

## Choice
A Windows toast with a short sound when a thread blocks or finishes and Frontier isn't focused; clicking it opens the thread.

## Reason
Not given.

## Consequences
- Differs from the recommendation. Phase 7 revisits the sound when voice decides the one audio source.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0407: herdr's toasts are off; the service raises every notification.
