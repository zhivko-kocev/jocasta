# 0303 Proactive interjections

- Status: decided
- Kind: product
- Phase/slice: 8/8.7
- Date: 2026-10-08

## Question
What does Jarvis raise on its own, and when does it cut in?

## Facts
- 8.2 ranks events and holds them in quiet hours (0298); 8.3 routes them (0299); the boss has a `speak` tool (0296).

## Options
| Option | Pros | Cons |
|---|---|---|
| Urgent events plus disk health; urgent cuts in, the rest waits for the end of the boss's turn; once plus one reminder after 30 minutes | — | — |
| Threads and pipelines only | Smaller | — |
| No reminders | Quieter | Things slip |

## Recommendation
As described.

## Choice
Sources: urgent events (a red pipeline on main or his branch, a blocked thread, a meeting in 5 minutes) and machine health (disk under 10% free). Urgent events cut in at once through 8.3; others wait for the end of the boss's current turn. Each is raised once, plus one reminder after 30 minutes if still unresolved. The boss may speak them (0296).

## Reason
Not given.

## Consequences
- Slice 8.7 is decided.
