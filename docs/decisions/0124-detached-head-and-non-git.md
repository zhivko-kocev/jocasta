# 0124 Detached HEAD and non-git projects

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
Where does Jarvis work on a detached HEAD, and in a non-git project?

## Facts
- Non-git folders can be projects through `project add` (0105). With 0117 alone, their project thread would be chat-only, so Jarvis could never edit them.
- A detached HEAD (bisect, tag checkout) has no branch to bind.
- What changed (0004): 0105 created non-git projects after the plan's options were written.

## Options
| Option | Pros | Cons |
|---|---|---|
| A detached HEAD uses the project thread; a non-git project's single thread has full tools | Non-git projects are usable | Non-git edits have no branch isolation |
| Both use the project thread, chat-only | One rule | Non-git projects are read-and-talk only |
| Refuse both | Simplest | No Jarvis during bisects or in notes folders |

## Recommendation
Detached HEAD to the project thread; non-git projects get full tools. This changes the plan's first option because of 0105.

## Choice
On a detached HEAD Jarvis opens the project thread, chat-only. A non-git project has one thread with full tools; the guard still applies from phase 2.

## Reason
Not given.

## Consequences
- The trunk rule (0117) applies to git projects only.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
