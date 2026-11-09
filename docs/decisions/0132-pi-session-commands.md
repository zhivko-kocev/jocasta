# 0132 Pi's session commands

- Status: dropped
- Kind: product
- Phase/slice: 1/1.5
- Date: 2026-10-07

## Question
What happens to Pi's `/new`, `/resume`, `/fork`, `/tree` and its session flags?

## Facts
Verified 2026-10-07 in Pi 1.0.1:
- Built-in slash commands are dispatched before extension commands (`dist/modes/interactive/interactive-mode.js:2499-2637`), and an extension command with a built-in's name is skipped (`:446-458`); they can't be hidden or renamed.
- `session_before_switch` (reason `new` or `resume`), `session_before_fork` and `session_before_tree` can cancel them (`dist/core/extensions/types.d.ts:567-640`; `examples/extensions/confirm-destructive.ts`).
- The entry reads argv before Pi's `main()`.
- `/tree` moves within one session file.
- This answers plan 5.4's phase 1 item on overriding session commands.

## Options
| Option | Pros | Cons |
|---|---|---|
| Cancel `/new`, `/resume` and `/fork` with a pointer to threads; keep `/tree`; the entry rejects `--continue`, `--resume`, `--session` and `--fork` | No second conversation; rewind inside a window still works | The commands still show in autocomplete |
| Block all four | Strictly linear threads | No rewind in a window |
| Leave them | No code | Sessions come back |

## Recommendation
Block `/new`, `/resume` and `/fork`; keep `/tree`.

## Choice
Now (0307): dropped; Pi's `/new`, `/resume` and `/fork` work as Pi ships them.

Originally: A Jarvis extension cancels `/new` and `/resume` (`session_before_switch`) and `/fork` (`session_before_fork`) with a one-line pointer to threads; `/tree` stays. The entry rejects `--continue`, `--resume`, `--session` and `--fork` with a message.

## Reason
Not given.

## Consequences
- Messages, as previewed: `Jarvis has no sessions. Start work with /thread new "<title>"; this thread continues.` and `jarvis: --resume isn't used; opening a folder continues its thread.`
- Threads can branch inside a window through `/tree` (L20's question).
- The entry passes `--continue` itself (0129); only flags he types are rejected.
- Amended by 0307: dropped in the pruning pass.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
