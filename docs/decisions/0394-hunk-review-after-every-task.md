# 0394 hunk review after every task

- Status: v1
- Kind: feature
- Phase/slice: program
- Date: 2026-10-13

## Question
What shows the diff after every task (0380), and who comments?

## Facts
Verified 2026-10-13.
- hunk (`modem-dev/hunk`) 0.23.0, MIT, Bun + OpenTUI; npm `hunkdiff-windows-x64@0.23.0`; `hunk diff <range> --watch`, `hunk session comment apply --repo <path> --stdin`, `hunk session comment list --repo <path> --json`, `hunk skill path`.
- omp ships a `reviewer` subagent; he called it 138 times.

## Options
| Option | Pros | Cons |
|---|---|---|
| hunk beside the thread, omp's reviewer after every task | Line-level, keyboard-driven, Windows-native, no fork; findings on their lines | A terminal pane, not the canvas |
| whiteboard's semantic diff | AST-aware, tours | Tied to the canvas |
| Reviewer only on `/review` | Fewer model calls | Review is not automatic |

## Recommendation
hunk beside the thread, omp's reviewer after every task.

## Choice
v1: when a workspace thread finishes, a hunk pane opens beside it (`hunk diff <trunk>...<branch> --watch`), the thread is asked to run omp's reviewer and post findings as hunk comments, and the boss hears "review ready: N notes". hunk ships as the pinned npm platform binary; the layout comes from the UX pass (0395).

## Reason
"hunk beside the thread"; "omp's reviewer after every task".

## Consequences
- Replaces 0380's open details; R's grilling is not needed.
- Amended by 0425: the thread runs the tests before the reviewer.
