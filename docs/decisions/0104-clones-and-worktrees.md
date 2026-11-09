# 0104 Clones and worktrees

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
Are clones and worktrees with the same identity one project or several?

## Facts
- Git worktrees share one common dir (`git rev-parse --git-common-dir`); opencode v1.18.22 resolves every worktree of a repo to one project (`packages/core/src/project.ts:114-120`).
- The migration ledger plans `catalog.db` v1 as "projects and paths".
- 1.4 puts workspace threads in worktrees beside the main root, so worktrees must belong to their project.

## Options
| Option | Pros | Cons |
|---|---|---|
| One project, many paths | Threads, memory and scopes follow the repo from any clone; worktrees attach | Needs a main path per project |
| Worktrees join; clones are separate | No main-path rule for clones | Two catalog entries for one repo |
| Every path separate | Simplest catalog | Each worktree would be its own project, against 1.4 |

## Recommendation
One project, many paths.

## Choice
One project with many paths. Each path is `main`, `worktree` or `clone`; the main path is the first one found under a watched root, and it can be changed.

## Reason
Not given.

## Consequences
- `catalog.db` v1 holds projects and their paths.
- The main path anchors the trunk and the worktree dir (1.4).
