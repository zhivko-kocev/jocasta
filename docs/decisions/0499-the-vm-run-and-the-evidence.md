# 0499 Everything runs for real, with an evidence pack per chapter

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-27

## Question
"Full bachelors with real value" reads as: run everything for real. That means the clean-machine tests in the VM (0481); every pre-registered benchmark at full size (0482: 30 tasks × 3 runs × 5 arms, after the 25-trial pilot shown to him first, then his 10-task case study); the statistics; and results, tables and charts in `D:/thesis`. What comes on top?

## Facts
Verified 2026-10-27.
- FINKI's thesis template forbids AI writing or significantly helping to write the text; 0483: agents never write thesis or slide text, they prepare the kit.
- The full run takes days, mostly waiting on quota (0482's pause at 85%).
- The design's optional user study (8–15 people with SUS and NASA-TLX) is outside 0482, needs people he recruits, and needs FINKI's rules for studies with people.

## Options
| Option | Pros | Cons |
|---|---|---|
| An evidence pack per chapter | Each outline chapter gets its findings, numbers, tables, figures and sources, as notes, not sentences | Agent time |
| A fact-check of his drafts | Later, every claim and number he wrote is checked against the data and sources; nothing is rewritten | Waits on his drafts |
| A user study | The strongest evidence for a developer tool | Outside the pre-registration; he recruits; FINKI's rules for studies with people |
| Nothing more | Just the full runs and their results | — |

## Recommendation
None: a pick-any list.

## Choice
The full run as described, plus an evidence pack per chapter and, once he writes, a fact-check of his drafts. Everything is documented, including his philosophy and the case against it, which he writes first. No user study.

## Reason
"i want everything documented + my philosophy and against it i will start with that as we have full working v1 tested pen tested ultimate tested :D"

## Consequences
- The packs live in `D:/thesis/evidence/` (outside the repo, 0483), one per chapter group plus `philosophy.md`, and their sources join `D:/thesis/sources/bibliography-ieee.md`.
- His review of v1 and the cutover (0487) come after the run.
