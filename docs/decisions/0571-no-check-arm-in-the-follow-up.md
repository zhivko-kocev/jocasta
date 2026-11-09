# 0571 The follow-up study drops f-check-off; A6's check stays as the product behaves

- Status: v1
- Kind: process
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
The benchmark's fixture repos have no mise check task, so the layer's `check` gate finds nothing to run in A6. Keep A6 as built and drop `f-check-off`, or give each task a visible check?

## Facts
Found 2026-11-09 in Phase D's build.
- The check runner runs a project's own mise `verify`/`test` task plus the thread's `done_when` commands; a project without either gets no gate, and its review turn runs the tests as before.
- Each task's `test_command` runs only its hidden test files (`D:/thesis/data/bench/tasks/README.md`), so the layer can't use it without naming them.
- 0543's follow-up study compares A6 with `f-index-off`, `f-check-off`, `f-prewalk`, `f-keepalive`, `f-keep-context` and `f-ponytail-if`, 10 tasks × 2 reps each.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep A6 as built; drop `f-check-off` | A6 is the product on a repo without a mise check; no new task data; 20 trials fewer | The check gate isn't measured in the task run, only in H8 |
| A visible check per task, passed as `done_when` | The gate works in A6 and `f-check-off` measures it | 40 commands to write and validate in the VM; only the layer gets them |

## Recommendation
Keep A6 as built; drop `f-check-off`.

## Choice
A6 runs as built: its `check` tool answers "No checks" on a fixture without a mise check. `f-check-off` leaves the follow-up study, which keeps five comparisons (120 trials). Cached checks are measured in H8 (shared check hits).

## Reason
"Keep as built; drop f-check-off".

## Consequences
- Amends 0543 (the follow-up's arms) and 0542 (the `check` tool isn't switched off by any arm).
- The `check` tool's off switch goes with the arm.
