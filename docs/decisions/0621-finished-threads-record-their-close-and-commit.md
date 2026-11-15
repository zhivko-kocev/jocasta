# 0621 A finished thread records when it closed and its commit; the weekly review gets production numbers

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-15

## Question
The factory text asks for the economics of each unit and a loop where every job improves the next; the SDLC text asks that a merged commit leads back to the session that made it. What does Jocasta keep about a finished thread, and who reads it?

## Facts
Verified 2026-11-15 (`docs/research/state-of-the-art.md`, section 14).
- **`thread.json`** (`src/frontier/server/task-file.ts`) holds the thread's id, start time, spend, its session file, the plan's approval, the report and every review with its checks. A finished or removed thread is marked `closed`, with no time. The live install's two threads are both open.
- **Nothing adds finished threads up.** The benchmark's H8 reports cost per finished thread; the product does not.
- **The squash commit** (`finish.ts`) carries the thread's title and "Squash-merged from <branch>:" with the branch's subjects. The branch is deleted. Nothing leads from the commit to the thread, its task file or its session.
- **The weekly review** (0583) already runs one deterministic step before its agent: the scorecard (0616), kept per ISO week in Jocasta's data folder and given to the agent with last week's.
- **His projects include client repositories**, whose history other people read.

## Options
| Production numbers | For | Against |
|---|---|---|
| v1 | numbers gather before the benchmarks; no model call | one more step in the weekly review |
| After v1 | nothing changes before the VM weekend | no data until then |
| Drop | nothing to build | the loop stays without numbers |

| From a commit to its thread | For | Against |
|---|---|---|
| The commit's id in `thread.json` | nothing written into the project's history | lost with the data folder |
| A trailer in the squash commit | survives the data folder | shows the tooling in every client repository |
| Both | both | both |
| Neither | nothing to build | no trace |

## Recommendation
v1; the commit's id in `thread.json`.

## Choice
- **`thread.json` records when a thread closed and how:** merged, opened as a PR, or removed.
- **It records the commit:** the squash commit's id for a merge, the pushed head for a PR. Nothing is added to the commit's message.
- **The weekly review computes production numbers before its agent runs,** with no model call, from the project's `thread.json` files: threads finished that week (merged and as PRs), lead time from start to close, spend per finished thread, and how many passed their first review. A removed thread is not a finished one.
- **The numbers are kept per ISO week** in `<data>/projects/<pid>/production/`, as the scorecard's are, and this week's and last week's are given to the review.

Preview of the lines, as he saw it:
```
Threads finished this week: 6 (5 merged, 1 PR)
Lead time, median: 3 h 10 min (40 min to 2 d)
Spend a finished thread, median: $4.20 (total $31.70)
Passed the first review: 4 of 6
```

## Reason
His answers: `v1` and `thread-json`.

## Consequences
- Amends 0540 (the task file's record) and 0583 (the weekly review).
- A thread closed before this build has no close time and is left out of the numbers.
- A week with no finished thread says so in one line.
- Plan appendix I gains the two fields and the `production` folder (0027).
