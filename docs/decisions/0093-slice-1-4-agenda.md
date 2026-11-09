# 0093 Slice 1.4 agenda

- Status: decided
- Kind: process
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
Which decisions does slice 1.4 (the thread model) grill, now that v4 7.2, F0 slice 6 and 3.6 are decided fresh (0088)?

## Facts
- Plan 1.4 listed ten items; items 5 and 6 carried v4 7.2 (reuse a clean checkout, else a linked worktree at `<main-root>.wt/<thread-id>`, never `--force`), v4 F0 slice 6 (rebind a renamed branch through the reflog) and v4 3.6 (close signals).
- The phase 1 exit smoke names branch `feat/add-retry` for a thread "Add retry"; no item decided how branch names are made.
- No lesson maps to 1.4.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of thirteen: hierarchy; names; trunk rule; trunk set; creating a workspace thread; branch name from the title; checkout; branch renamed; branch merged or deleted and what closing does; worktree cleanup after close; detached HEAD and non-git folders; thread ids and the dormant label; the schema | Each v4 bundle becomes a question; branch naming and cleanup are decided | Thirteen questions |
| The plan's ten | Fewer questions | Branch naming and cleanup are left to the build |

## Recommendation
The fresh agenda of thirteen.

## Choice
The fresh agenda of thirteen.

## Reason
Not given.

## Consequences
- Plan 1.4's list becomes the thirteen items, citing this record. Branch naming and worktree cleanup are new.
