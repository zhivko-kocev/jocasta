# 0195 Slice 2.7 agenda

- Status: decided
- Kind: process
- Phase/slice: 2/2.7
- Date: 2026-10-08

## Question
Which decisions does slice 2.7 (chat-only mode) grill, now that v4 5.6's read-only list is decided fresh (0088)?

## Facts
- Chat-only applies to the project thread on trunk (0117), a detached HEAD (0124) and the home thread (0125).
- Lesson L36: changing the tool list breaks the prompt cache; denying at call time keeps it stable.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of five: what is allowed; read-only shell commands; hidden or declared-and-denied tools; the denial message; an escape hatch | The cache cost is asked explicitly | — |
| Change it | — | — |

## Recommendation
The fresh agenda.

## Choice
The fresh agenda of five.

## Reason
Not given.

## Consequences
- Plan 2.7's list becomes the five items, citing this record.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
