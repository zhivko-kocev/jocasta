# 0598 The git guard is off until omp's git builtin runs git on Windows

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-10

## Question
In Jocasta's panes on Windows, omp's bash tool can't run git: every git command the guard lets through fails with exit 127. What does Jocasta do until omp fixes it?

## Facts
Verified 2026-11-10 on his PC with omp 18.18.6 (`@oh-my-pi/pi-natives` 18.18.6) and Git for Windows 2.54.0.
- With `bash.gitGuard: true` the bash tool passes `PI_GIT_GUARD=1` to its embedded shell (`exec/bash-executor.ts:737-739`), and that shell then has a `git` builtin (`type -t git` prints `builtin`). Its refusals print as before, but every command it lets through fails: `bash: line 1: C:\Program Files\Git\cmd\git: No such file or directory`, exit 127 (the path without `.exe`).
- With the guard off (`PI_GIT_GUARD` in neither the shell's env nor the process's), `type -t git` prints `file` and `git --version` runs; the same with a minimal Windows env.
- `git.exe …`, a full path to git, and git inside `pwsh -c` start the binary, which the builtin doesn't guard (`bench/h1-guard/git-guard.ts`); `config/guard.yml`'s patterns still apply to them.
- The 18.18.2 and 18.18.6 spike runs (`git-guard-spot.ts`, 0589) showed exit 127 on every line the guard allowed; it was taken for the harness.
- His sessions: bash git ran until 2026-11-09 (over 10,000 calls, on earlier omp versions); since then 21 calls failed this way, 6 of them in the two threads, which went on with `git.exe` or PowerShell.
- `config/guard.yml` turns the guard on in every project, thread and boss pane (0398, 0450).
- Without the guard, the patterns still deny force pushes, `reset --hard`, `checkout --`, `restore .`, `clean -`, `switch -f`, `branch -D`, `worktree remove -f` and the rest. Only the builtin refuses, in a main checkout, a plain `git stash`, a branch switch, `git restore <file>` and `git reset <commit>`; in a linked worktree (a thread's) it lets those run and keeps the discarded changes for recovery (omp's changelog).
- H1 scores the guard file it's given (`--guard`, by default `config/guard.yml`); on omp 18.18.6, 30 of its 1,486 verdicts come from the git guard (`D:/jocasta-spikes/omp-18.18.6/seams.md`). `git-guard-check.ts` runs omp's shell with the guard on and off itself.
- No omp issue covered it (can1357/oh-my-pi searched on 2026-11-10; #17322 asks for approvals of the guard's refusals).

## Options
| Option | Pros | Cons |
|---|---|---|
| Guard off until omp fixes it, and an issue on omp | git works in every pane; the patterns stay; the agents' `git.exe` way around the builtin skips it anyway | Until the fix: no refusal of stash, branch switches, `restore <file>` and `reset <commit>` in a main checkout, and no recovery copies in a worktree |
| Guard on, issue only | The refusals stay for an agent that doesn't find a way around | Every pane loses turns to exit 127, and agents go around it |
| Nothing now | — | As above, and no fix asked for |

For H1, if omp isn't fixed by the weekend:

| Option | Pros | Cons |
|---|---|---|
| The guard as it ships that day | Measures what the panes had; no new file | Not comparable with the earlier runs' git-guard verdicts |
| A snapshot with the guard on (`bench/h1-guard/guard-0580.yml`) | Comparable with the earlier runs | Not the guard the panes had |

## Recommendation
Guard off until omp fixes it, and an issue on omp; H1 measures the guard as it ships.

## Choice
- `config/guard.yml` sets `bash.gitGuard: false` until an omp release runs git through the builtin on Windows; the bump to that release (0383) turns it back on, after checking that git runs with it on.
- The agent files the issue on omp's GitHub with placeholder paths only: [can1357/oh-my-pi#18722](https://github.com/can1357/oh-my-pi/issues/18722).
- H1 measures the guard as it ships that day; while the git guard is off, H1's report says so and why. `git-guard-check.ts` still measures omp's git guard.

## Reason
His answers: "Guard off until omp fixes it" and "The guard as it ships that day".

## Consequences
- Amends 0398 and 0450: `bash.gitGuard` is off for now.
- The VM's Jocasta trial panes run git normally while it's off.
- Builder's choices, listed as built (0363): the comment above the key in `config/guard.yml`; the guard test checks the rules' order and uniqueness without pinning the key; README's config section and the plan's guard paragraph say it's off and why.
