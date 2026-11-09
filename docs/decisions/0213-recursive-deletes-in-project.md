# 0213 Recursive deletes inside the project

- Status: decided
- Kind: product
- Phase/slice: 2/2.2
- Date: 2026-10-08

## Question
What happens to a recursive delete inside the project or worktree?

## Facts
- Recursive deletes outside the project are denied (0210). Tracked files can be restored from git; untracked work can't.
- Asks hold in `yolo` (0199).

## Options
| Option | Pros | Cons |
|---|---|---|
| Allow | Cleanup works | Untracked files are lost |
| Ask | He sees each one | Prompts during cleanup |
| Allow paths git ignores, ask otherwise | Cleanup flows; real work is protected | A git check per delete |

## Recommendation
Allow ignored paths, ask otherwise.

## Choice
Allowed when every target is ignored by git (`git check-ignore`); otherwise asked.

## Reason
Not given.

## Consequences
- None.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
