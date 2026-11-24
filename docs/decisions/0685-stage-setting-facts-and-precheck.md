# 0685 Stage setting: a facts block by code and the full check ahead of time

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
Threads and reviewers spend turns on `git status`, `git log` and `git diff --stat`, and wait for the full check at report time. What does code hand them?

## Facts
- One PR-review thread cost $70.25; `thread_facts.rs` and `thread_start.rs` build prompts; `checks.rs` keeps a tree-keyed check store.

## Options
| Option | Pros | Cons |
|---|---|---|
| The model runs git itself (today) | No code | Tokens, time, inconsistent facts |
| A facts block computed by code and the full check run during the turn-end quick check's slack | Cheaper turns; `report_result` answers from the store | More code |

## Recommendation
Facts by code; precheck.

## Choice
His answer of 2026-11-24:
- `thread_facts.rs` gains `start_facts(place)` (branch, trunk, `HEAD` as 7 hex, which of `quick`, `verify`, `test` exist), appended to the task prompt in `thread_start.rs`, and `review_facts(place, range, run)` (up to 30 lines of `git log --format="%h %s"`, up to 60 of `git diff --numstat` then `… <n> more files`, each check's command, exit and tree key), placed in `review_prompt` after its lead line. Each block starts `Facts as of <sha7> (computed by Jocasta; don't re-run git status, git log or git diff --stat for them):`. A block over 60 lines is written to `<thread dir>/facts.md`; the prompt carries its first 40 lines and `Full facts: <path>`.
- Precheck: in `pane_check` for a thread's pane, when the `Quick` run passes (or the tree was already checked) and the `Full` specs differ, a `run_checks(place, full_specs, Some(tree))` is spawned on `prechecks`; its result lands in the tree-keyed store so `report_result` and the review answer from it. A precheck's failure isn't sent to the pane. A newer tree for the pane cancels the running precheck. The check queue's memory reserve applies.

## Reason
Fewer turns, an earlier verdict.

## Consequences
Depends on 0677's check depth.
