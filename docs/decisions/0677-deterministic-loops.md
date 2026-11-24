# 0677 Deterministic validation loops

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Amends: 0592
- Date: 2026-11-24

## Question
Today the model decides when to run checks, fills in the files and tests of its report, and an idle pane runs a check typed in as a prompt. How do the loops become code?

## Facts
- The `pane-check` role starts a 70.4 ms process each turn end; one PR-review thread cost $70.25.
- Compaction was set at 300k (0448, 0592); the doctor report §7.3 row 4 measured at most $27.95 saved over 1.9 weeks and advised keeping 300k.
- `ReviewFlow::checks_for` (`review_flow.rs:637`), `pane_check` (782), `report_result` (`tools.rs:610`), `Finisher::finish` (`finish.rs:121`), `wording.rs:23–28, 38, 48` exist.

## Options
| Option | Pros | Cons |
|---|---|---|
| Model-run checks and model-filled reports (today) | No new code | Costs tokens; reports can be wrong; idle checks race the screen |
| Turn-end `quick` check, a full-check gate in `report_result`, 3 same-gate failures = stuck, PR by code, state on rows | Checks cost no model turn; a stored report is true; loops end by count | More code in the service |
| Keep compaction at 300k | Measured saving small | He wants it earlier |

## Recommendation
The code owns every gate; compaction at 200k.

## Choice
His answer of 2026-11-24 (his call stands against the doctor report's advice):
- Check depth: `ReviewFlow::checks_for` takes `Quick` (the project's trusted mise task `quick`, else `verify`, else `test`; no `done_when`) or `Full` (today's behaviour: `verify` else `test`, plus `done_when`). Jocasta starts no containers (0682).
- Turn end in every pane: `pane_check` also serves thread panes (skipped when the thread's phase isn't Task or a plan awaits) and uses `Quick`; `idle_check`, `start_idle_check` and the typed `IDLE_CHECK_FRAME` path go; the frame text stays as the `additionalContext` wrapper; the task set is kept as `prechecks`.
- `report_result` first runs the `Full` checks; a failing gate returns `report_result: not stored, the checks fail:\n<gate_reply>\nFix them and hand the report in again.` On a pass `ThreadReport.files` is `git diff --name-only <trunk>...HEAD` and `ThreadReport.tests` one `ReportedTest` per gate; `files` and `tests` leave the tool's schema and `WHEN_YOU_FINISH`.
- Loop cap: per pane the last failing gate command, a count and its last 20 output lines; the same gate failing a third time in a row trips the breaker: the turn-end `stop_result` context is empty, in `report_result` the error is `` report_result: `<gate>` failed 3 times in a row; this thread is stopped until he answers. ``, the service sends `abort`, the thread's stuck state reads `stuck: <gate> failed 3×`, a NEEDS YOU event is raised. Only his `thread_send` or a new task resets it.
- PR by code (`Mode::Pr`, after Allow): origin `github.com` → `git push -u origin <branch>` then `gh pr create --base <trunk> --head <branch> --title <task title> --body-file <temp>`; `dev.azure.com` or `*.visualstudio.com` → `az repos pr create --org … --project … --repository … --source-branch … --target-branch … --title … --description …`; any other host keeps today's `pr_request` prompt. Body: the report's `summary`, `## Checks` (command and exit per gate), `## Commits`. A failing command's stderr tail (20 lines) is the finish error; nothing is retried.
- Rows: the bridge's `todo` and `usage` show after the title as `4/9 · 62%` (an unknown part omitted).
- Compaction `thresholdTokens` 300000 → 200000 in `config/omp/config.yml`, `config/boss.yml` and `src/frontier/server/overlay.rs`.

## Reason
The model writes code; the service checks it.

## Consequences
- Amends 0592 (the compaction threshold).
- Journal `gate` lines (0688); the full check runs ahead of time (0685).
- Retires typed idle checks.
