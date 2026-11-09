# 0457 Worktree setup: copied files and per-thread variables

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
A new thread's worktree has no gitignored files such as `.env`, and parallel threads share ports and Compose names. What does Jarvis set up before omp starts?

## Facts
Verified 2026-10-23.
- Nothing is copied today: `worktree.create`, then `layout.apply` runs omp (`herdr.ts:632-646`).
- Claude Code ships `.worktreeinclude`: gitignore syntax; it copies only files that match and are gitignored (https://code.claude.com/docs/en/worktrees).
- herdr plugins can hook `worktree.created` (socket-api.mdx:505, 577), but that hook runs apart from Jarvis's `layout.apply` and could race omp's start [INFERENCE].
- Pane env already flows to each thread (`herdr.ts:525-527, 646`; `paneEnv` holds hunk vars only, `service.ts:417`).
- Ports, the database and Docker collide across worktrees ("Port conflicts hit you first", https://developer.upsun.com/posts/ai/git-worktrees-for-parallel-ai-coding-agents); Compose defaults the project name to the folder's basename unless the compose file sets `name:` (https://docs.docker.com/compose/how-tos/project-name/).

## Options
| Option | Pros | Cons |
|---|---|---|
| Jarvis copies `.worktreeinclude` matches and sets per-thread env | Claude Code's format; done before omp starts | Jarvis code in thread start |
| A herdr `worktree.created` plugin | No Jarvis code in the start path | May race omp's start |
| Nothing | No change | Threads start without `.env`; ports collide |

## Recommendation
Jarvis copies `.worktreeinclude` matches and sets per-thread env.

## Choice
After `worktree.create` and before omp starts, Jarvis copies the files a project lists in its `.worktreeinclude` (Claude Code's format: gitignore-style patterns of gitignored files such as `.env`) from the main checkout into the thread's worktree. Every thread pane gets per-thread environment variables for a Compose project name and a port offset (names chosen by the builder, listed as built).

## Reason
"Jarvis copies `.worktreeinclude` matches and sets per-thread env".

## Consequences
- Amends 0412 (thread start) and 0445 (what a new worktree holds).
- Dependency installs stay with the thread.
- Projects opt in by adding `.worktreeinclude` and reading the variables; plan appendix I lists the variable names as built.
- Amended by 0473: for a trusted mise config, `mise env --json` joins the thread pane's env at start and on respawn.
- Amended by 0477: the per-thread variables reach the project's Jarvis-kept mise config through `MISE_ENV`.
