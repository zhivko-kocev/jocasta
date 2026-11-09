# 0073 When containers run

- Status: decided
- Kind: product
- Phase/slice: 0/0.7
- Date: 2026-10-04

## Question
When do containers start and stop?

## Facts
No resource exists yet; the first is speech-to-text in 4.8.

## Options
| Option | Pros | Cons |
|---|---|---|
| Started when first needed, stopped after an idle time | No idle cost | First use waits |
| Always on while Frontier runs | Instant | Idle memory |
| Started with the machine | Instant from boot | Most idle cost |

## Recommendation
On demand.

## Choice
Started when first needed and stopped after an idle time; each resource's slice sets its idle time.

## Reason
Not given.

## Consequences
None beyond the choice.
