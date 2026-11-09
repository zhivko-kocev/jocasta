# 0246 Ask tool

- Status: decided
- Kind: product
- Phase/slice: 3/3.6
- Date: 2026-10-08

## Question
Which `ask` tool does Jarvis have, and what happens in headless runs? (fresh agenda of one question, 0088)

## Facts
Verified 2026-10-08: the installed Pi package ships `examples/extensions/question.ts` (286 lines) and `questionnaire.ts` (448 lines, several questions with tab navigation), MIT.
- omp's `ask` has options, multi-select and a recommended index; his usage: 96 calls (appendix A). 0141 made resumable headless runs v1.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's questionnaire plus omp's `recommended` and `multi` fields | One tool; an overlay in the tab | — |
| Pi's `question.ts` as it is | Smallest | One question per call; no recommendation |
| Also move resumable runs to after v1 | — | — |

## Recommendation
Pi's questionnaire plus omp's fields.

## Choice
Pi's questionnaire adapted with `recommended` and `multi`; an overlay in the tab. In a headless run a question stops the run with exit 3 and is stored on the thread; opening the thread (`jarvis --thread <id>`, 0139) shows it, and his answer continues the run.

## Reason
Not given.

## Consequences
- This is 0141's resumable-run design for questions; approvals stay denied headless (0207).
- From phase 4, an unanswered question marks the thread blocked and reaches the boss (4.6).
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
