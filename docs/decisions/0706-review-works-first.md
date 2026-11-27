# 0706 Review: works first

- Status: v1 (built 2026-11-27)
- Kind: product, prompts
- Phase/slice: ultron 0.3 iteration, M2 R2
- Date: 2026-11-26
- Amends: 0680 (the pull-request review runs the project's check before its first turn), 0678 (a `works` section)

## Question
A review today reads the diff and hands in findings; whether the change works at all is a model's judgement, and tests run "only when the change can't be judged without them". A change that doesn't build or doesn't do what it says wastes every reviewer turn spent on its style. What does a review do first?

## Facts
- Jocasta's check runner already runs a thread's project check (`verify`, else `test`) and `done_when` by code before a thread's review (0542, 0546); a pull-request review (0680) runs nothing.
- Other people's code must run only in the clean room (0708); his own threads' checks run on the worktree while he works.
- A tool omp mounts runs inside omp's bash call, whose own deadline then applies (0675's `LONG_RUN_NOTE`).
- `done_when` commands are judged by the shipped guard before they're stored: deny refuses, prompt needs his Allow (0540, 0580).

## Options
| Option | Pros | Cons |
|---|---|---|
| Code runs the check first; then the reviewer tries the feature with `review_try`, whose steps Jocasta runs and judges; any failure ends the review | A broken change costs no reviewer turn; "it works" is shown, not claimed | Reviews of slow projects wait for their check |
| Keep the reviewer judging, tests on its own | Nothing to build | Spends turns on changes that don't run |

## Recommendation
Works first, by code.

## Choice
His answers of 2026-11-26 and the approved plan:
- Pull-request reviews: before the review thread's first model turn Jocasta runs the project's check through the clean room (0708) on the PR head. When the clean room isn't possible because Docker isn't running, the runs are off or the image can't be pulled, the Allow box asks `Run PR #<n> by <author> on this machine?` once per project and author; the answer is kept in `projects.yaml` as `prRun: {<repo folder>: {<author>: allow|deny}}` (keyed like `push:`, 0613). Allow runs the check in the review's worktree on his machine; deny continues the review with `works: not run`. A failing check ends the review at once: the thread goes to `ready` with a single P0 card `It doesn't work: <check> failed` whose proof is the output tail, and no reviewer turn is spent.
- Built detail: a project without a `verify` or `test` task asks nothing (there is nothing to run); the row says why.
- New thread tool `review_try {what, steps: [{run, expect}], screenshots?: [path]}` for review threads and a thread's reviewer. Jocasta runs each `run` itself: in the clean room for a pull request (on his machine when he allowed it), in the worktree for his own threads, guard-checked like `done_when` (deny refuses; a prompt asks his Allow). A step passes when its output contains `expect`, or, for `expect: "exit:N"`, when it exits N. Screenshots (PNG, JPEG, WebP or GIF files, at most 10 MB each, checked by their first bytes) are copied to `<data>/briefs/assets/<brief id>/` and served by the brief routes at `/brief/assets/<brief id>/<name>`. A failed step answers `review_try: step <i> didn't give "<expect>": exit <n>: <tail>` and ends the review like a failed check: a P0 card `It doesn't work: <what> failed` is kept, and any later `review_note` or `review_try` is refused with `the review ended: <what> failed`.
- Every check and step becomes a row of the new section `works` (label, state `passed` | `failed` | `not_run`, duration, output tail, screenshots), kept in the review's notes file and shown first on thread and pull-request pages.
- `config/pr-review.md` orders the review: read; `review_try` first (commands for a CLI or library; for a web UI start it, drive it with the browser headless, save screenshots); stop when it fails; only then findings. Its old step 5 ("run tests only when…") goes. The thread reviewer's prompt (`review_prompt.rs`) gets the same order.
- His own threads: `thread_finish` runs the clean-room check on the head commit before the Merge/PR question, whatever the door (kept per tree, 0708). Failing: `Finish: the clean-room check fails: <tail>` and no question. Not possible: Finish goes on and the thread's page says `clean-room run not possible: <reason>`.

## Reason
A review starts from proof that the change works; a model turn is spent only on a change worth judging.

## Consequences
- New `steps.rs`, the one executor of `{run, expect}` steps for `review_try`, finding proofs (0707) and the clean-room check.
- `model.rs`: `Section::Works`, `WorksRow`, `WorksState`; `Card.ran` (the step that proves it). `review_notes.rs`: `ReviewNotes.works`.
- `pr_review.rs` runs the check before the brief goes in; `finish.rs` runs it before the question; `tool_specs.rs` and `tools.rs` add `review_try` (a long-running tool).
- `projects_file.rs` reads and writes `prRun`.
