# 0359 Workspace threads commit and report

- Status: decided
- Kind: product
- Phase/slice: 3/3.7
- Date: 2026-10-09

## Question
Nothing told a workspace thread to commit and call `report` when done. What tells it?

## Facts
- Found in the v1 beta (2026-10-09): with a real model a boss-started thread made its edit, never committed and never called `report`, so it ended idle: no done toast, no spoken report, and the boss never heard. Only the `report` tool's description mentioned it.
- Pi adds a tool's `promptGuidelines` to the system prompt whenever the tool is active.

## Options
| Option | Pros | Cons |
|---|---|---|
| A guideline on the `report` tool, for every workspace thread | Covers threads he starts too | One more prompt line |
| The same line only in briefs the boss writes | Smaller | Threads he starts still go idle silently |

## Recommendation
None given; both shown with the wording.

## Choice
The `report` tool's prompt guideline, in every thread that has the tool: "When the work is done, commit it on this thread's branch and call report with the outcome; if you can't finish, call report with status failed or blocked and say why."

## Reason
Not given.

## Consequences
- Slice 3.2's report-back text gains the guideline.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
