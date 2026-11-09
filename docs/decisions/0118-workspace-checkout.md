# 0118 Workspace checkout

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
Where does a workspace thread work?

## Facts
Probed 2026-10-07 with git 2.54.0 in a temp dir.
- `git worktree add -b feat/add-retry <root>.wt/t-9f3e2a1b main` created the worktree and branch.
- `git worktree remove` refused a worktree with untracked files unless `--force`.
- The sample `projects.yaml` ignores `*.wt` (0109).

## Options
| Option | Pros | Cons |
|---|---|---|
| Reuse a clean checkout that holds the branch, else a linked worktree at `<main-root>.wt/<thread-id>`; never `--force` | Parallel threads never share a checkout; worktrees sit beside the repo | A sibling `.wt` folder per repo |
| Worktrees under the data dir | Nothing beside the repos | Deep paths on Windows; far from the repo |
| Bind a branch only, with no worktree | No extra folders | One workspace thread per checkout; switching under a running tab breaks it |

## Recommendation
Reuse a clean checkout, else `<main-root>.wt/<thread-id>`.

## Choice
A workspace thread reuses a clean checkout (the main path or a clone, 0104) that already holds its branch; otherwise Jarvis creates a linked worktree at `<main-root>.wt/<thread-id>`. Jarvis never passes `--force`.

## Reason
Not given.

## Consequences
- Worktree paths are recorded as `worktree` paths of the project (0104).
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
