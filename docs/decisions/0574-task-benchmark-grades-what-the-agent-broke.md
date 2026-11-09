# 0574 The task benchmark grades what the agent broke, has limits, and holds its conditions

- Status: v1
- Kind: benchmark
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
The review found that the task harness turns failures the agent caused into infra errors, has no time limit on grading, lets harnesses update themselves mid-run, lets two benches share a folder, and grades Python in the agent's own venv. How does the run grade, bound and hold its conditions?

## Facts
Verified 2026-11-09 by the review (0573), report EVL.
- Grading's setup failure throws (`evals/runner.ts:626`) and `classifyInfra` files it as `setup` infra (`evals/tasks.ts:467`); a spike where the agent rewrote a dependency file came back as infra, not as a failed trial. 4 of 10 main tasks build in their setup.
- runner's `git()` has no `maxBuffer` (Bun stops at 1,114,112 bytes with ENOBUFS); four `JSON.parse(line)` calls throw on a torn last line; grading clones the trial branch by name, so a fix left on another branch grades 0 (EVL-8).
- `spawnSync` for setup and hidden checks has no timeout (runner.ts:605), and the worker per trial has no timer; the slowest gold grading took 1,521 s.
- Each arm's `--version` is read once per bench start (bench.ts:700-711); opencode 1.18.22 and Claude Code 2.1.296 update themselves unless `OPENCODE_DISABLE_AUTOUPDATE` / `DISABLE_AUTOUPDATER` is set, and nothing sets them.
- A resume checks only the trial order (bench.ts:263); `--timeout` defaults to 60 while 0508 says 90; 0530's stopped job left a bench running beside the next.
- Python grading reuses the trial's `..\venv` (tasks.ts:378-379), the one the agent worked in.

## Options
| Item | Options (pros / cons) |
|---|---|
| EVL-4 grading failures | (a) grade 0, confirmed on a clean clone of the start commit (precise / one more setup run on failure) · (b) every failure after the harness grades 0 (simplest / a network flake counts against the arm) · (c) as now (none / survivorship bias, up to 3× quota) |
| EVL-5 grading limit | (a) fixed 60 min for setup + hidden checks (simple, recorded / one number for every task) · (b) per task, max(30 min, 3 × gold) (tight / needs validation timings) · (c) share the 90-minute trial budget (one limit / cuts a slow correct trial) |
| EVL-7 harness updates | (a) disable auto-update in a4/a7 and re-read `--version` after every trial (closes it / a variant change) · (b) refuse at startup while auto-update is on (no arm change / config locations vary) · (c) a manual VM step (nothing to build / unchecked) |
| EVL-11 run folder and conditions | (a) `run.lock`, stored conditions, `--timeout` 90 (closes 0530's case / more checks) · (b) lock only (smallest / conditions drift) · (c) as now |
| EVL-18 Python grading venv | (a) fresh `<root>/gvenv` per trial (fully clean / +1–4 min per Python trial) · (b) record `pip freeze` and flag a difference (cheap / flags, doesn't prevent) · (c) keep and document |

## Recommendation
EVL-4 (a) with the internal parts; EVL-5 (a); EVL-7 (a); EVL-11 (a); EVL-18 (b).

## Choice
- EVL-4: a failure during grading, after the harness, grades 0 once a clean clone of the start commit confirms it: when the setup passes there, the change broke it and the row counts as failed with `hidden: setup failed: 0`; when it fails there too, the row is infra. The internal parts land with it: `maxBuffer` on the runner's `git()`, lenient line parsing, and grading the trial's HEAD rather than the branch by name (EVL-8).
- EVL-5: one fixed grading limit, 60 minutes for setup and the hidden checks together, run with an async spawn and the product's process-tree kill; a killed check grades 0; the limit is recorded in `conditions.json`.
- EVL-5's other half (asked during the build, 2026-11-09): the task's setup before the harness gets its own 60-minute limit with the same tree kill; a setup cut there is an infra error, retried like the others; `conditions.json` records the setup limit beside the grading limit.
- EVL-7: `evals/variants/a4.yaml` gets `env: { DISABLE_AUTOUPDATER: "1" }` and `a7.yaml` `env: { OPENCODE_DISABLE_AUTOUPDATE: "1" }`; each arm's `--version` is read again after every trial, and on a change that arm's remaining trials stop (as A7's spend cap stops them).
- EVL-11: `<out>/run.lock` holds the pid and start time; a second bench beside a live holder is refused, and the lock is taken over once its pid is gone. The plan stores the trial limit, the profile and each arm's harness version, and a resume that differs is refused. `--timeout` defaults to 90.
- EVL-18: a fresh grading venv per Python trial, `<root>/gvenv`.

## Reason
"(a)" for EVL-4, EVL-5, EVL-7 and EVL-11; for EVL-18 "(a) A fresh grading venv (`<root>/gvenv`) per trial".
For the setup before the harness he chose "60 min, then infra" (the recommendation) over 20 minutes and no limit.

## Consequences
- Differs from the recommendation on EVL-18: a fresh venv instead of recording `pip freeze` and flagging a difference; each Python trial's grading takes 1–4 minutes longer.
- Amends 0482 (the infra rules: E1's clean grading now separates agent-caused setup failures from infra), 0508 (`--timeout 90` is now the default, not only the full run's flag), 0543, and 0500/0537 (the pinned harness versions are held by the variants' env and checked after every trial).
- `conditions.json` gains the grading limit; a4 and a7 gain their auto-update variables.
- `conditions.json` also gains the setup limit (the amendment above).
