# 0270 Working by hand in a boss-run thread

- Status: decided
- Kind: product
- Phase/slice: 4/4.7
- Date: 2026-10-08

## Question
What happens when he types in a thread the boss started?

## Facts
- Pi delivers messages typed during a run as steering (0267).

## Options
| Option | Pros | Cons |
|---|---|---|
| Both steer; the boss's messages are labeled | No handover | Two voices |
| He takes control until he hands it back | Clear ownership | A handover step |
| Read-only while the boss runs it | Simple | V12 limited |

## Recommendation
Both steer.

## Choice
Both steer: his and the boss's messages arrive in order, and the tab marks the boss's as from the boss.

## Reason
Not given.

## Consequences
- None.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
