# 0564 Version control returns: git in the repo, the agent commits finished phases under a repo-only identity, a public `jocasta` repository, whiteboard as a submodule, and the folder renamed after the build

- Status: decided
- Kind: process
- Phase/slice: program
- Vision: —
- Date: 2026-11-09

## Question
He asked to "introduce version control and rename the project to jocasta". How does history start, who commits, where is it published, what happens to `forks/whiteboard`, and when is `D:/jarvis` renamed?

## Facts
Verified 2026-11-09.
- `D:/jarvis` had no `.git` since before 0417; 0003 has him commit at each phase-end review and AGENTS.md forbids agents any git write; 0383 has him add submodules; 0417 kept `forks/whiteboard` a plain clone of v0.2.4 "until git returns"; 0480 left renaming the folder and the GitHub repository to him.
- None of his 54 GitHub repositories is named `jarvis` or `jocasta`; `zhivko-kocev/jocasta` didn't exist.
- His global git `user.email` is a work address whose domain is in his private-names list; every commit carries its author and committer address, which a content scan doesn't see.
- `.gitignore` already leaves out `node_modules/`, `dist/` and `.bun-cache/`; the rest is about 18 MB.
- Windows won't rename a folder that a running process uses as its working directory; this session and the build's agents run in `D:/jarvis`. omp's session history and project memory are keyed by the folder path.
- A full copy of the repo before the harness-OS build is at `D:/jocasta-pre-harness-os` (without `node_modules`, `forks`, `dist`).

## Options
| Item | Options (pros / cons) |
|---|---|
| History | Baseline = the pre-build copy (the build as one diff) · First commit after the build (no baseline) |
| Commits | He commits at phase ends (0003) · The agent commits finished phases |
| GitHub | Later, by him · Create `jocasta` now |
| whiteboard | Submodule now · He adds it (0383) |
| Folder | After the build, by a script he runs · Pause and rename now |
| Identity | GitHub noreply · His global work address · An address he names |

## Recommendation
Baseline commit; he commits; GitHub later; submodule now; rename after the build; GitHub noreply.

## Choice
- `git init -b main` in the repo now; the first commit comes after the whole harness-OS build passes `mise run check`.
- From then on the agent commits each finished phase once its gates pass (no push of unscanned history, no branches unless asked); he reviews the commits.
- Commits carry a repo-only identity: `user.name` "Zhivko Kocev", `user.email` his personal address set in the repo's own config; his global config is untouched.
- `zhivko-kocev/jocasta` is created public now (empty, remote `origin`); every push first runs the private-names scan over tracked files and checks that every author and committer address in `git log` is the repo identity.
- `forks/whiteboard` is a submodule (`https://github.com/devdotfast/whiteboard`, at `v0.2.4`).
- After the build the agent writes a script that renames `D:/jarvis` to `D:/jocasta` and runs `mise trust` there; he closes the session and runs it.

## Reason
"First commit after the build"; "the agent just i need to see what user conf i have for this project"; "Create `jocasta` now"; "Submodule now"; "After the build, by script"; "set local user conf onnly for this porject on" his personal address.

## Consequences
- Amends 0003 (the agent commits finished phases), 0383 (the agent added this submodule), 0417 (whiteboard is a submodule) and 0480 (the agent creates the repository and scripts the folder rename).
- AGENTS.md's "No git writes" rule changes to say what agents may do.
- After the rename, omp's session history and memory for the project start fresh under the new path.
