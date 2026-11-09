# 0125 The home thread

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
What does `jarvis` do in a folder that is not a project?

## Facts
- No auto-registration (0102); home and drive roots are never projects (0106).
- Principle 9: the harness stands alone. V9: no sessions.
- The 0.8 smoke asks "who am I, and who are you?" in an empty temp folder (0076).
- From phase 4 the boss is the global conversation.
- The question was missing from 1.4's agenda and was asked under 0020.

## Options
| Option | Pros | Cons |
|---|---|---|
| One continuous, chat-only home thread for every non-project folder | Questions anywhere; nothing lost; no stray projects | A third thread kind; files outside projects can't be edited |
| Refuse with a hint to `project add` | Nothing unthreaded | No quick questions outside projects |
| An unthreaded Pi session with full tools | Most freedom | Sessions come back (V9); full tools anywhere |

## Recommendation
One chat-only home thread.

## Choice
Outside every project, `jarvis` opens the home thread: one continuous thread of kind `home`, chat-only (enforced by the guard from phase 2), stored in `catalog.db`.

## Reason
Not given.

## Consequences
- Opening screen, as previewed: `Jarvis — home thread (not a project; chat only)` and a tip to run `jarvis project add <path>`.
- `catalog.db` v1 gains a `threads` table with the 0126 columns for the home thread.
- Thread kinds are `project`, `workspace` and `home` (0114 adds the third kind).
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
