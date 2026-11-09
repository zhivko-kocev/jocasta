# 0120 Thread names

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
What are the two kinds of thread called?

## Facts
- V9: a project is "one continuous thread"; "a workspace may be a thread". V10: "subthreads … name and shape to be decided".
- Section 7's vocabulary used "project thread" and "workspace thread", pending this answer.

## Options
| Option | Pros | Cons |
|---|---|---|
| Project thread and workspace thread | Says what each is bound to | Long; commands shorten to `thread` |
| Thread and subthread | V10's word; short | Says nothing about branch or worktree |
| Project thread and task thread | Says what it's for | Collides with todos (3.2) and subagent tasks (6.4) |

## Recommendation
Project thread and workspace thread.

## Choice
"Project thread" and "workspace thread"; commands say `thread`.

## Reason
Not given.

## Consequences
- Section 7's vocabulary is confirmed.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
