# 0546 The ledger shows peak context, and checks answer as gates with a rerun command

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-08

## Question
Should the ledger (0538) show how full a thread's context got, and how do the cached checks (0542) answer a thread and the review?

## Facts
Verified 2026-11-08.
- Each request's uncached input + cache read + cache write is the context's size at that request; omp's session file has them per message, and the model's context window is known.
- HumanLayer aims for 40–60% utilization; Jocasta compacts at 300k tokens (0448) and doesn't measure the peak.
- His client project's preflight answers `PASS|FAIL` per gate with the command that fixes a failure, so the agent doesn't reason about readiness.
- The cached runner (0542) knows each check's command, its failures (from JUnit) and its log; the narrowest rerun is the test runner with the failing files.

## Options
| Item | Options (pros / cons) |
|---|---|
| Peak context | In `thread_status`'s line and the benchmark (the boss can say it) · Benchmark only |
| Check replies | Gates with a rerun command · Failures and the log path only (0542) |

## Recommendation
In the line; gates.

## Choice
- The ledger keeps each thread's peak context; `thread_status`'s line becomes:
  `spend $1.84 · peak context 41% (82k of 200k) · cache read 71% · 3 re-reads · 1 cache miss · 2 compactions`;
  the benchmark rows carry it.
- Checks answer as gates, and say nothing when every gate passes:
  ```
  checks: FAIL (tree 3f9a1c)
    PASS  lint       mise run lint
    FAIL  test       mise run test
          2 failed: test/auth.test.ts:42 "retries back off", test/auth.test.ts:77 "gives up after 5"
          log: <data>/checks/3f9a1c/test.log
          rerun: bun test test/auth.test.ts
    PASS  done_when  bun run e2e:login
  ```

## Reason
"Yes, in the line"; "Gates, the preview".

## Consequences
- Amends 0538 (the line) and 0542 (the reply's form).
- A rerun line is given only when the runner can name the failing files; otherwise it repeats the check's command.
