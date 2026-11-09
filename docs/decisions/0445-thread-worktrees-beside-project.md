# 0445 Thread worktrees beside the project

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
A thread's worktree failed to check out a project whose files reach 168 characters: "Filename too long". Where does Jarvis put a thread's worktree?

## Facts
Verified 2026-10-15.
- herdr's default worktree folder is `~/.herdr/worktrees/<repo>/<branch>`: on his machine 96 characters before the file path, so a 168-character file reaches 264, over Windows' 260; `core.longpaths` is off and Windows' `LongPathsEnabled` is 0.
- His own worktrees sit beside the repo (`D:\<repo>-<name>`, about 25 characters) and check out fine.
- herdr's `worktree.create` takes a `path` per call (`WorktreeCreateParams.path`, herdr 0.9.5); a branch slug is at most 40 characters (0414).
- The watched-roots scan already files a linked worktree under its project's paths, wherever it is.

## Options
| Option | Pros | Cons |
|---|---|---|
| Beside the project, `<repo>.wt` | Same drive as each repo; nothing to ask; any machine | A `.wt` folder beside each project |
| Siblings like his | His own naming | Many threads fill the parent folder |
| One folder, asked in setup | One place | One more question; no public default |
| herdr default + longpaths | No change in Jarvis | Builds may still fail past 260 |

## Recommendation
Beside the project, `<repo>.wt`.

## Choice
`thread_start` creates a thread's worktree at `<project's parent>/<project folder>.wt/<title slug>` (e.g. `D:\acme-api.wt\add-rate-limits`); when that folder exists the slug gets `-2`, `-3`… Threads made before keep their folders.

## Reason
"Beside the project, <repo>.wt".

## Consequences
- Amends 0412 (herdr's default worktree folder).
- Plan appendix I names the folder.
- Amended by 0455: the branch takes the same `-2`, `-3`… suffix when `feature/<slug>` exists, and a new thread starts a fresh session.
- Amended by 0457: a new worktree gets the project's `.worktreeinclude` files and per-thread variables.
