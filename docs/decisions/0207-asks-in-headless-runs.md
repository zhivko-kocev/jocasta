# 0207 Asks in headless runs

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
What does an ask rule do in an unattended run?

## Facts
- Exit code 3 means an approval was needed and denied headless (0140); resumable runs come with 3.6 (0141).
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Deny the call and exit 3 | Predictable | The run stops |
| Allow | Runs on | Defeats the rule |

## Recommendation
Deny and exit 3.

## Choice
The call is denied and the run stops with exit 3, until 3.6's resumable runs let it wait for an answer.

## Reason
Not given.

## Consequences
- None.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
