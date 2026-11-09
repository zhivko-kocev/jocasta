# 0382 Beta-2 suggestions

- Status: decided
- Kind: product
- Phase/slice: 2/2.2, 4/4.9
- Date: 2026-10-10

## Question
Which of the v1 beta 2's suggestions (0374) are built: new guard rules, the duplicate blocked toast and the docs drift?

## Facts
Verified 2026-10-10.
- The git-guard (0209–0215) has no rule for `git stash drop` or `git stash clear` (stashed work is then recoverable only from dangling commits), `git reflog expire` or `git gc --prune=now` (they remove the safety net behind reset and rebase), `gh repo delete`, or pipe-to-shell (`curl … | sh`, `wget … | bash`, `iwr … | iex`).
- A thread that blocks once can raise its blocked toast twice (0329: one toast per transition).
- `README.md` and `docs/plan.md` describe commands, flags, env vars and paths that have drifted from the build.

## Options
| Item | Options |
|---|---|
| `git stash drop`, `git stash clear` | ask; deny; leave allowed |
| `git reflog expire`, `git gc --prune=now` | ask; deny; leave allowed |
| `gh repo delete` | deny; ask; leave allowed |
| Pipe-to-shell | ask; deny; leave allowed |
| Tick-list (0089) | the duplicate blocked toast; the docs drift |

## Recommendation
Ask, ask, deny, ask; both ticked.

## Choice
- `git stash drop` and `git stash clear` ask.
- `git reflog expire` and `git gc --prune=now` ask.
- `gh repo delete` is denied.
- Pipe-to-shell (`curl … | sh`, `wget … | bash`, `iwr … | iex`) asks.
- The duplicate blocked toast is fixed: one toast per blocked transition (0329).
- The docs drift is fixed: claims in `README.md` and `docs/plan.md` are checked against the build and each mismatch is fixed in the doc; a mismatch where the build is wrong goes to him (0020).

## Reason
All as recommended.

## Consequences
- Rule ids, denial texts and corpus cases follow 0209–0215's formats; each rule gets a block or ask case and a near-miss allow case.
- Amends 0374's last consequence: these suggestions are now decided.
- As built (0363): rules `git-guard/stash-drop`, `git-guard/reflog-expire` (also `gc --prune=now`), `git-guard/gh-repo-delete` and `git-guard/pipe-to-shell` (sh, bash, zsh, dash, ksh, fish, pwsh, powershell or `iex` reading from curl, wget, `iwr` or `irm`); the corpus has 182 cases.
- As built: the duplicate came from a second question while the thread already waited on one; the server now sends the blocked toast and its spoken line only on the transition into blocked, and the second question is a card and a boss line.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
