# 0399 One continuous thread per worktree

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
How does a worktree's single omp session stay one continuous thread, and when does a thread end?

## Facts
Verified 2026-10-13.
- `/handoff` and auto-compaction write a compaction entry into the same session file; `omp --continue` reopens the newest session in the folder (`docs/handoff-generation-pipeline.md:113`).
- Auto-compaction tries `compaction.methodOrder` in order (default `remote, snapcompact, handoff, shake, soft`); `shake` elides heavy tool results into recoverable `artifact://` references without a model call and falls through to the next method when it can't free enough (`docs/compaction.md:200-204`).
- Idle compaction (`compaction.idleEnabled`, default off; `idleTimeoutSeconds` 300, `idleThresholdTokens` 200000) runs the same method order (`session/session-maintenance.ts:2025-2027`).
- `plan.defaultOnStartup` opens new sessions in plan mode and leaves resumed ones as they were; `--config <file>` layers settings onto one run.
- herdr has `worktree.remove`; closing a tab keeps the worktree and its session.

## Options
| Option | Pros | Cons |
|---|---|---|
| Handoff first at 60% + task-end `/shake` from the companion | Richest summaries | Jarvis code sends `/shake` |
| Shake first, handoff second, idle on | All omp settings, no Jarvis code; cheap shakes after each task | Fewer summaries |
| Soft first | In-place summary | Thinner than a handoff |

## Recommendation
Handoff first + task-end `/shake`.

## Choice
`compaction.methodOrder: [shake, handoff, soft, snapcompact, remote]`, `compaction.thresholdPercent: 60`, `compaction.idleEnabled: true` with `compaction.idleThresholdTokens: 100000` (timeout stays 300 s); `compaction.dropUseless` stays on. Jarvis starts project and thread panes with its per-project overlay (0400), which also sets `plan.defaultOnStartup: true`, so a new thread opens in plan mode and a reopened one keeps its mode. A thread lives until he removes it: closing the tab keeps the worktree, and reopening it runs `omp --continue` there.

## Reason
"the reccomended + shake after a task is done if that makes sense i think shake is a good tool investigate and see if we can use multiple approaches", then "Shake first, handoff second, idle on"; "Lives until you remove it".

## Consequences
- Differs from the recommendation (handoff first).
- Shake and snapcompact entries carry no text summary; the watcher skips them (0389).
- Amended by 0448: the pane settings carry the compaction block, with `compaction.thresholdTokens: 300000` instead of `thresholdPercent: 60`; the task-end `/handoff` carries a focus line.
- Amended by 0592: `compaction.methodOrder` is `[handoff, remote, soft, shake, snapcompact]`; on his models a shake rewrites the warm cache and blocks the handoff behind it.
