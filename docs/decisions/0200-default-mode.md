# 0200 Default mode

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
Which mode does a tab start in?

## Facts
- omp defaults to `yolo`, and he ran it that way. 0198 assumed asking for unmatched calls.
- Unattended runs can't answer asks in any mode (0140).

## Options
| Option | Pros | Cons |
|---|---|---|
| Normal, with `yolo` by key or flag | A fresh install never runs unknown commands unasked | A switch per tab |
| `yolo` by default | How he works in omp | Everything not denied or asked runs |

## Recommendation
Normal by default.

## Choice
`yolo` by default.

## Reason
Not given.

## Consequences
- With the default mode, unmatched calls run; 0198's ask applies in normal mode. The key and flag that switch modes are decided with the build's key bindings (0005) in 2.4.
- Deny rules and the git-guard (2.2) are the floor in `yolo`.
- Differs from the recommendation.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
