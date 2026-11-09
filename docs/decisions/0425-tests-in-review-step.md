# 0425 Tests run in the review step

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
Slice H (0375) and his research idea #1: verify before a thread reports done?

## Facts
Verified 2026-10-14.
- After a task (0394) the thread runs omp's reviewer and posts notes to hunk; nothing runs the tests.

## Options
| Option | Pros | Cons |
|---|---|---|
| Add tests to the review step | Catches broken work; one turn | One more step per task |
| A separate verify thread | Independent | Slower, costs more |
| Not now | Nothing to build | Done may mean broken |

## Recommendation
Add tests to the review step.

## Choice
In the review step the thread first runs the project's tests (as its AGENTS.md or package scripts say), then the reviewer; the boss hears "review ready for <title>: N notes, tests pass" (or "tests fail", or "no tests").

## Reason
"Add tests to the review step".

## Consequences
- Amends 0394.
- Closes his research idea #1 (verify before done).
- Amended by 0453: tests pass only on an exit-0 run in the session; the reviewer gets the plan and rules; size, test edits and blocking findings are shown; a project's `verify` skill runs.
- Amended by 0460: the reviewer runs on a GitHub Copilot GPT model and reports only material findings.
- Amended by 0484: after the review, a `thread_finish` that merges or opens a PR waits for his Allow in the companion.
