# 0454 Merging after main moved: bring it in, re-test, re-review

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
A thread's review tests its branch alone. What does `thread_finish` merge do when main gained commits after that review, and what does a thread branch from?

## Facts
Verified 2026-10-23.
- `merge()` checks a dirty main checkout, the branch and conflicts but not whether main moved since the review (`service.ts:1844-1860`); the review covered `trunk...branch` at review time (`service.ts:1215`).
- Jarvis passes no `base` to `worktree.create` (`herdr.ts:631-638`), so a thread branches from whatever the main checkout has checked out, though `service.ts:615` says threads branch from trunk.
- herdr's `worktree.create` takes `base` and "creates the branch from the requested base or `HEAD`" (herdr 0.9.5 `src/api/schema/worktrees.rs:22`; socket-api.mdx:344).
- "A green run only proves the tree it ran on" (Superpowers `finishing-a-development-branch`, https://github.com/obra/superpowers/blob/main/skills/finishing-a-development-branch/SKILL.md).

## Options
| Option | Pros | Cons |
|---|---|---|
| Bring main in, re-test, re-review | Only tested combinations reach main | A merge can take another review turn |
| Refuse and tell him | Simple | He starts the update himself |
| Merge as today | No change | An untested squash after another thread's merge |

## Recommendation
Bring main in, re-test, re-review.

## Choice
Threads start from the project's main branch (`worktree.create` gets `base`). At `thread_finish` merge, if main moved since the thread's last review, Jarvis merges main into the thread's branch, re-runs the review step (tests and review), and merges only when it passes; a conflict or a failed review stops and tells him.

## Reason
"Bring main in, re-test, re-review".

## Consequences
- Amends 0426 (the merge path).
- "Passes" follows 0453 (checked tests, no blocking finding).
- The `pr` finish is unchanged.
