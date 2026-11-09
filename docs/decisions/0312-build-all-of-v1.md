# 0312 Building all of v1 without phase stops

- Status: decided
- Kind: process
- Phase/slice: all (0003, 0086)
- Date: 2026-10-08

## Question
What does "implement everything" cover, and do phase ends stop for review?

## Facts
- His words: "we should implement everything". The grilling program and pruning pass are complete (0306–0311). 0003: he reviews and commits at each phase end; phase 0's review hasn't happened yet. The agent never commits.
- Installs still ask; Jarvis runs only in temp fixture repos until phase 2's guard lands (0026).

## Options
| Option | Pros | Cons |
|---|---|---|
| Build all of v1 now, phases 1–8 in order | — | — |
| Bring after-v1 items into v1 too | — | Reopens every deferral |
| Both, after-v1 right after v1 | — | — |
| Follow-up: stop at each phase end for review and commit | His rules (0003) | Waits between phases |
| Follow-up: run through phases without stopping | Faster | Larger reviews |

## Recommendation
Build v1 now; stop at each phase end.

## Choice
The build covers every v1 decision of phases 1–8, in phase order, starting with phase 1's fact check. After-v1 items stay out. Phases run on without stopping for review; he reviews and commits whenever he chooses.

## Reason
"we should implement everything"

## Consequences
- Amends 0003: phase-end reviews no longer block the next phase.
- Installs still ask; the fixture gate (0026) holds until phase 2's guard lands.
- Differs from the recommendation on the review gate.
