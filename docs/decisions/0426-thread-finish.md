# 0426 thread_finish: merge or PR on his word

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
Slice H (0375): the finish flow after a reviewed thread.

## Facts
Verified 2026-10-14.
- Today nothing merges, opens a PR or removes the worktree; the boss manual forbids pushing or merging without his word; herdr has `worktree.remove`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Boss thread_finish: merge or PR, on his word | One sentence finishes a thread | Pushes need `gh` and his login |
| Merge only | No GitHub | No PRs |
| Leave it manual | Nothing to build | Hand work each time |

## Recommendation
Boss thread_finish.

## Choice
A boss tool `thread_finish(thread, how)` with `how` = `merge` or `pr`, called only on his word: `merge` has the thread squash-merge its branch into trunk in the main checkout; `pr` has it push the branch and open a PR with `gh`; then the service removes the worktree and its workspace through herdr. A failed merge or push leaves everything in place and tells the boss why.

## Reason
"Boss `thread_finish`: merge or PR, on your word".

## Consequences
- Closes slice H of 0375 with 0425.
- As built: the service runs the squash merge itself (so the refusals are checked reliably); the thread pushes and opens the PR for `pr`.
- Amended by 0453: `thread_finish` says when the last review wasn't a pass.
- Amended by 0454: threads branch from main; a merge after main moved brings main in, re-runs the review step and merges only on a pass.
