# 0414 Thread branches are feature/<slug>

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
What branch does a new thread's worktree get?

## Facts
Verified 2026-10-13.
- F2 built the bare title slug; 0121 named workspace-thread branches `feature/<slug>`.
- herdr puts worktrees under `~/.herdr/worktrees/<repo>/`.

## Options
| Option | Pros | Cons |
|---|---|---|
| feature/<slug> | As 0121 | Longer |
| Bare slug | Shorter | Differs from 0121 |

## Recommendation
feature/<slug>.

## Choice
`thread_start` creates branch `feature/<title slug>`.

## Reason
"feature/<slug>".

## Consequences
- Amended by 0455: when `feature/<slug>` exists, the new thread's branch takes `-2`, `-3`…
