# 0129 Opening continues

- Status: decided
- Kind: product
- Phase/slice: 1/1.5
- Date: 2026-10-07

## Question
What does opening a folder do?

## Facts
Verified 2026-10-07 in Pi 1.0.1 (`dist/cli/args.js:56-100,290-296`): Pi starts a new session on each launch unless `--continue` (the most recent in the session dir) or `--session`; `--session-dir <dir>` beats `JARVIS_CODING_AGENT_SESSION_DIR` and the setting.
- V9: "There is no resuming or switching."

## Options
| Option | Pros | Cons |
|---|---|---|
| Always continue, with no picker: trunk opens the project thread, a workspace thread's checkout opens that thread, anything else opens the home thread | V9 as written | A fresh context comes only through rollover or a new workspace thread |
| Keep Pi's session commands | Familiar | Sessions come back |

## Recommendation
Always continue.

## Choice
The entry passes `--session-dir <thread dir> --continue`. `jarvis` on trunk in a project opens the project thread; in a workspace thread's worktree or checkout, that thread; outside every project, the home thread (0125).

## Reason
Not given.

## Consequences
- A non-trunk branch with no thread is 0130.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
