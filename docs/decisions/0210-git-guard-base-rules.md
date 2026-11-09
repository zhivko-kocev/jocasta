# 0210 git-guard base rules

- Status: decided
- Kind: product
- Phase/slice: 2/2.2
- Date: 2026-10-08

## Question
Which built-in rules deny in every mode, and which ask?

## Facts
- His rules file (0067) tells the agent never to commit, push, merge, rebase or delete branches unless asked.
- 0122's close flow deletes branches with safe `git branch -d`.
- His own `!` commands meet deny rules but not asks (0202).
- Asked as a multi-select: ticked rules deny, unticked ones ask.

## Options
| Option | Pros | Cons |
|---|---|---|
| Force push, including `--force-with-lease` and `+refspec` | — | — |
| Plain `git push` | — | — |
| `reset --hard`, `clean -f`, `checkout -- .` | — | — |
| `branch -D`, `push --delete` | — | — |
| Recursive deletes outside the project | — | — |
| Disk tools: `mkfs`, `dd of=/dev/…`, `format`, `diskpart` | — | — |

## Recommendation
None was marked in the question.

## Choice
Now (0218): force push with lease asks; `--force`, `-f` and `+refspec` deny.

Originally:
Deny in every mode: force push (including `--force-with-lease` and `+refspec`); `reset --hard`, `clean -f` and `checkout -- .`; recursive deletes outside the project or worktree (`rm -r`, `rd /s`, `Remove-Item -Recurse`); disk tools (`mkfs`, `dd of=/dev/…`, `format`, `diskpart`).
Ask in every mode: plain `git push`; `branch -D` and `push --delete`.

## Reason
Not given.

## Consequences
- His own `!git push` runs (asks don't apply to `!`, 0202); `!git push --force` is denied.
- 2.2's disagreement items (`rm -r` without `-f`, `sudo`, `mkfs`, `dd`, asking before `git add`) are partly answered: disk tools deny; the rest are asked next.
- Amended by 0218: force push with lease asks instead of denying.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
