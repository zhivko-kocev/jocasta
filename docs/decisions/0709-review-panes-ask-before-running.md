# 0709 Review panes ask before running

- Status: v1 (built 2026-11-27)
- Kind: product, security
- Phase/slice: ultron 0.3 iteration, M2 R4
- Date: 2026-11-26
- Amends: 0680 (`config/review.yml` gains approval rules and a headless browser)

## Question
A pull-request review thread reads a stranger's code in a worktree on his machine, under omp's default `yolo`. Jocasta now runs the change itself, in the clean room (0706, 0708). Should the review pane's own bash still run whatever the model types?

## Facts
- omp 19.2.1 (`omp://approval-mode.md`, `omp://tools/bash.md`, and the bash tool's `approval` in the shipped binary): `bash.patterns` rules are `{match, approval: allow|prompt|deny}`, first match wins; an overlay's list is put ahead of earlier layers', so the guard, loaded last, decides first. An explicit `prompt` asks even under `yolo`. An `allow` applies only to a simple command; a compound line (`|`, `;`, `&&` without `bash.allowCompoundCommands`) is judged by its `deny` and `prompt` rules alone.
- A line made only of mounted tool commands is `read` for bash, but a matching rule still decides first: without an `allow` for them, a catch-all `prompt` would ask for every `review_note`.
- A user policy `tools.approval.bash: prompt` would also ask for those tool lines (omp's resolver applies it to `read` decisions that carry no policy), so it can't express "reading runs, tools run, the rest asks".
- `find -delete|-exec|-ok|-fprint`, `rg --pre`, `git … --output` and `git grep -O` write files or run programs although their command reads.
- `browser.headless` is an omp setting.

## Options
| Option | Pros | Cons |
|---|---|---|
| `bash.patterns`: prompts for the writing forms, allows for reading commands and Jocasta's tools, then `*` prompt | Reading never asks; everything else asks him; the guard still decides first | Compound reading lines (`git log | head`) ask too |
| `tools.approvalMode: always-ask-cached` (the plan's contingency) | One setting | Asks for file reads, edits, subagents and every tool, Jocasta's included |
| Leave `yolo` | Nothing to build | A stranger's code can make the pane run anything |

## Recommendation
The `bash.patterns` allow-list; the contingency isn't needed.

## Choice
His answers of 2026-11-26 and the approved plan: `config/review.yml` keeps `plan.defaultOnStartup: false` and adds:
- `browser.headless: true`;
- `bash.patterns`, in order: `prompt` for `find * -delete*`, `find * -exec*`, `find * -ok*`, `find * -fprint*`, `rg *--pre*`, `git * --output*`, `git grep *-O*`, `git grep *--open-files-in-pager*`; `allow` for `git diff|log|show|status|ls-files` (bare and with arguments), `git blame *`, `git grep *`, `rg *`, `grep *`, `ls`, `ls *`, `cat *`, `head *`, `tail *`, `wc *`, `find`, `find *`, and Jocasta's tools `check`, `history_search *`, `review_note *`, `review_try *`; then `*` → `prompt`.
- Built detail: the allow-list is expressible in omp's settings, so `tools.approvalMode: always-ask-cached` isn't used.

## Reason
The reviewer reads freely and runs the change only through Jocasta; anything else it wants to run is his call.

## Consequences
- `mise run omp-seams` passes with the new keys (omp 19.2.1, 101 keys).
- `guard.rs`'s tests check the review pane's verdicts for simple commands after the guard.
- A review pane asks for compound reading lines; the model can use omp's read and grep tools instead.
