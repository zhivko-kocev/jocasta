# 0237 Chat-only allowed tools

- Status: decided
- Kind: product
- Phase/slice: 2/2.7
- Date: 2026-10-08

## Question
What does a chat-only thread allow?

## Facts
- Chat-only covers the project thread on trunk (0117), a detached HEAD (0124) and the home thread (0125).
- Verified 2026-10-08: Pi 1.1.0's read-only built-ins are `read`, `grep`, `find` and `ls` (`dist/core/tools`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's read-only tools plus Jarvis's non-code tools as later phases add them | The project thread keeps its state doc and memory | — |
| Pi's read-only tools only | Smallest | No state doc on the project thread |

## Recommendation
As recommended.

## Choice
Pi's `read`, `grep`, `find` and `ls`, plus Jarvis's own tools that don't change files as later phases add them (`ask` 3.6, state doc and todo 3.1–3.2, memory phase 5, web 6.9). `edit`, `write` and every other tool that changes files are denied.

## Reason
Not given.

## Consequences
- Shell commands are 0238.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
