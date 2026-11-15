# 0613 A push to main, master or develop asks him once per repo

- Status: v1
- Kind: product, security
- Phase/slice: program
- Date: 2026-11-14

## Question
His rule: "push on main and develop not possible unless explicitly approved by the user". The rule does not hold in a pane today. How is it enforced, how long does one approval last, and where is it kept?

## Facts
Verified 2026-11-14 on his PC (git 2.54.0, omp 18.20.0); the scratch tests are in `D:/jocasta-spikes/sota/`, outside the repo.
- **The shipped guard today**, by its own matcher: `git push origin main`, a bare `git push` on main, `git push origin develop` and `git push origin HEAD:develop` are allowed. Only `HEAD:main` and `:master` are denied (0580). The guard matches a command's text, so it cannot see which branch a plain `git push` updates. omp's git guard (back on with 0600) does not cover pushes.
- **git can take a hook from configuration**, and configuration can come from the environment: `hook.jocasta.event=pre-push` and `hook.jocasta.command` through `GIT_CONFIG_COUNT`, `GIT_CONFIG_KEY_n` and `GIT_CONFIG_VALUE_n`. A `pre-push` hook is given the remote's address and the branches the push really updates.
- **Tested in a scratch repo:**
  - a push that updates main or develop was refused in every spelling tried: plain, a bare `git push` with an upstream, `git.exe … :main`, an alias, `HEAD:develop`. Feature-branch pushes passed;
  - the repo's own hooks kept running, in `.git/hooks` and under a husky-style `core.hooksPath`;
  - a repo-level setting that disables the hook did not turn it off;
  - about 30 ms a push.
- **Two flags get past it:** `--no-verify` and `-c hook.jocasta.enabled=false`. The guard can deny both by text. A script that clears the pane's environment, or omp's `eval`, is 0485's accepted limit: everything runs as him.
- **A git older than 2.54 ignores these keys in silence.**
- **Push commands in his sessions since 2026-11-01:** at most 132, in 21 sessions and 76 bursts (pushes less than 10 minutes apart count as one). 54 name main or master, 43 push the current branch without naming it, 36 came from subagents. The count includes pushes to scratch remotes that tests create on this disk.
- **The Allow box** (`src/frontier/server/allow.ts`) is the one `thread_finish` uses: a popup with a typed 3-letter code (0558), answering Deny by itself after 2 minutes.
- **`~/.jocasta/agent/projects.yaml`** is his file, and Jocasta already writes it (`jocasta project add`).

## Options
| Question | Options | Recommended |
|---|---|---|
| How it is enforced | The hook asks him through the Allow box; the hook always refuses; omp's own prompt on every push | The hook asks |
| How long one Allow lasts | 10 minutes for that pane and branch; once per repo until Jocasta closes; once per repo for good; every push | 10 minutes |
| Where a per-repo answer is kept | Both answers in `projects.yaml`; only Allow; in the database with a new command | Both, in `projects.yaml` |

## Recommendation
The hook asks; an Allow holds 10 minutes; then, once he chose per repo: both answers kept in `projects.yaml`.

## Choice
- **A git hook from configuration, injected through each pane's environment**, runs before every push from a project pane, a thread pane, the boss pane and their subagents.
- **Protected branches:** main, master and develop, and the project's own trunk when it has another name.
- **A push that updates a protected branch on a network remote** (`https`, `ssh`, `git@host:`) is checked. A remote that is a folder on this disk passes. Other branches pass.
- **The first such push in a repo asks him** with the Allow box, naming the repo, the branch, the commits and the pane that asked.
- **His answer is kept for that repo** in `projects.yaml`, in a `push:` map from the repo's folder to `direct` or `pull-requests`:
  - Allow writes `direct`: later pushes to that repo's protected branches pass unasked;
  - Deny writes `pull-requests`: later pushes to them are refused unasked, and the agent is told this repo takes pull requests;
  - no answer in 2 minutes, or Jocasta not running, refuses that push and keeps nothing.
- **He changes his mind by editing or deleting the line.** A refusal names the file and the line.
- **The guard denies `--no-verify` on a push and `-c hook.jocasta.enabled=false`.**
- **Setup checks for git 2.54 or newer** and says so when it is older.

## Reason
His answers: "recommended but ask once per repo i guess or?", then "once per repo because on some repos you work on main or master or develop directly on others you open PRs so you cant know". The answer describes the repo, so both answers are kept.

## Consequences
- Differs from the recommendation on how long an Allow lasts: after `direct`, a stray push to that repo's main by any pane goes through.
- Amends 0580 (two guard rules more) and 0109 (`projects.yaml` gains the `push` map).
- Plan appendix I gains the hook's internal role, the service route, the pane environment entries and the `push` map (0027).
- A repo with no answer yet and no Jocasta running cannot push to a protected branch from a pane; he pushes from his own terminal.
