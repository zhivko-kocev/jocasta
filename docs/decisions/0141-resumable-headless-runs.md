# 0141 Resumable headless runs

- Status: decided
- Kind: product
- Phase/slice: 1/1.7
- Date: 2026-10-07

## Question
Is the catalog's "resumable questions and approvals in headless runs" row (0082) in v1?

## Facts
- Strands interrupts stop the loop with the pending item and resume from that point when it is answered (0082).
- Plan 3.6 had headless runs fail with an error; exit code 3 marks a denied approval (0140).
- The boss dispatches threads while he is away (4.5).

## Options
| Option | Pros | Cons |
|---|---|---|
| v1, designed in 3.6 | A boss-run thread that needs one answer resumes instead of restarting | Run state must survive between the stop and the answer |
| After v1 | 3.6 stays smaller | Blocked headless runs fail and re-run |
| Dropped | Simplest | The same, for good |

## Recommendation
None was marked in the question.

## Choice
v1. 3.6 designs how a headless run stops with a pending question or approval and resumes when it is answered.

## Reason
Not given.

## Consequences
- 3.6's agenda gains the resumable design; exit code 3 stays for runs that can't wait.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
