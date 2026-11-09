# 0417 whiteboard as a plain clone until git returns

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-13

## Question
How does whiteboard's source reach `forks/whiteboard` while the Jarvis folder has no git repository?

## Facts
Verified 2026-10-13.
- `D:\jarvis` has no `.git` today (`git status`: not a git repository), so `git submodule add` can't run.
- whiteboard v0.2.4 builds unpatched with pnpm 11.1.2 (corepack) and Node 24.13.

## Options
| Option | Pros | Cons |
|---|---|---|
| He restores git and adds the submodule; agents continue on a stand-in | As 0383 | Waits on him |
| A plain clone in forks/whiteboard now | F3 proceeds | He converts it to a submodule later |
| Pause F3 | Nothing temporary | F3 waits |

## Recommendation
Plain clone now.

## Choice
The agent clones `devdotfast/whiteboard` at tag `v0.2.4` into `forks/whiteboard` (a third-party clone; no Jarvis git write); he converts it to a submodule when the repository is back.

## Reason
"Clone into forks/whiteboard for now".

## Consequences
- Amends 0383's submodule step for whiteboard until git returns.
- Amended by 0564: git is back and `forks/whiteboard` is a submodule at `v0.2.4`.
