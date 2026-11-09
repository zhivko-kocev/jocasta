# 0114 Thread hierarchy

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
What shape do threads take?

## Facts
- V9: "A project is one continuous thread … A workspace may be a thread … main, master or dev is a scratch thread where he can only chat." V10: Frontier "can continue or activate a project's thread with subthreads".
- Worktrees belong to their project (0104).

## Options
| Option | Pros | Cons |
|---|---|---|
| A project thread plus workspace threads | Matches V9 and V10; trunk stays clean | Two kinds of thread |
| v5's flat model: one thread per branch, trunk a scratch thread with full tools | One kind | Against V9's chat-only main; no continuous project thread |
| One thread per project | Simplest | Parallel tabs share one conversation |

## Recommendation
A project thread plus workspace threads.

## Choice
Each project has one continuous project thread on trunk; it is chat-only and holds the running conversation, plans and decisions. Work happens in workspace threads, each bound to one branch and its own worktree; they report back to the project thread and close into it.

## Reason
Not given.

## Consequences
- The `threads` table has `kind` (`project`, `workspace`) and `parent_id` (1.4's item 13).
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
