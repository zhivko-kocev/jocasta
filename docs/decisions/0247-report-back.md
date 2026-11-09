# 0247 Report-back

- Status: decided
- Kind: product
- Phase/slice: 3/3.7
- Date: 2026-10-08

## Question
How does a workspace thread report back? (fresh agenda, 0088)

## Facts
- v4 7.4's result had status, a summary of at most 200 characters, commits, artifacts, follow-ups and a question.
- 3.1's state doc and phase 5's memory may not exist (0241), so consolidation on close has nothing to write into. Lean Jarvis (0229).

## Options
| Option | Pros | Cons |
|---|---|---|
| A small result posted as one message into the project thread and stored on the thread row | Small; V10, V11 | No artifacts field |
| v4's full result | Complete | Bigger |
| Free text | Smallest | Not structured for the boss |

## Recommendation
The small result.

## Choice
Now (0361): `status` also takes `blocked`, which ends the thread in the blocked state and relays its reason to the boss.

Originally:
A `report` tool the workspace thread calls once at the end, with `status`, `summary` (at most 200 characters), `commits` and `follow-ups`. The result is posted as one message into the project thread and stored on the thread's row; nothing else happens on close.

```
[report from t-3fa9c2e1 · feature/login-fix]
status: done
summary: Fixed token refresh race in login
commits: a1b2c3d, d4e5f6a
follow-ups: add test for expired refresh token
```

## Reason
Not given.

## Consequences
- The status values are those of 4.5's thread states that end a run: done, failed, cancelled.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
