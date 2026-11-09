# 0122 Worktree and branch cleanup

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
What happens to a closed workspace thread's worktree and branch?

## Facts
- `git worktree remove` refuses a worktree with changes unless `--force` (probed 2026-10-07), and 0118 never passes `--force`.
- His rules (0067, `~/.jarvis/agent/AGENTS.md`) tell the agent never to delete branches unless asked.
- `git branch -d` deletes only a branch merged into its upstream or HEAD; a squash-merged branch is not merged by that test (0119's probe).

## Options
| Option | Pros | Cons |
|---|---|---|
| Remove a clean worktree, keep the branch | Nothing lost; the agent rule holds | Merged branches pile up |
| Keep everything | No risk | `.wt` folders accumulate |
| Remove the worktree and the merged branch | Fully tidy | Deletes branches, which his rules forbid unless asked |

## Recommendation
Remove a clean worktree, keep the branch.

## Choice
On close, Jarvis runs `git worktree remove` (no `--force`) and then `git branch -d` on the thread's branch. A worktree with changes, or a branch git doesn't count as merged, is kept, and the close report says so.

## Reason
Not given.

## Consequences
- The deletion runs in Jarvis's close code after he confirms the close (0119); that confirmation is the asking that his agent rule (0067) requires. The rule itself is unchanged.
- Squash-merged branches stay, because `git branch -d` refuses them.
- Differs from the recommendation, which kept the branch.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
