# 0242 Smooth compaction spike rule

- Status: decided
- Kind: product
- Phase/slice: 3/3.4
- Date: 2026-10-08

## Question
Which compaction mechanism does Jarvis use?

## Facts
Verified 2026-10-08 in Pi 1.1.0 (`docs/compaction.md:23,37,47`): Pi's compaction makes a summary model call between turns, with prompt-cache writes off for that call; an extension can cancel it or supply its own result through `session_before_compact` (`dist/core/extensions/types.d.ts:588-597,1113-1116`).
- billion-context folds ranges in the model's own turn and needs a Node proxy (0216). The Jarvis rollover (state doc, briefing, recent tail, no model call) needs 3.1 and 3.3.
- Benchmarks: 0230. Lean Jarvis: 0229.

## Options
| Option | Pros | Cons |
|---|---|---|
| Spike: Pi's compaction, billion-context and the Jarvis rollover; the smallest within 1 point of the best pass rate wins | Decided by data; favors less code | A spike to run |
| Pi's compaction now | Smallest | A pause per compaction |
| The Jarvis rollover now | The plan's first option | The most code |

## Recommendation
The spike.

## Choice
Now (0323): billion-context (B) is chosen without the spike.

Originally: Built at 3.4, on Jarvis's evals and a Terminal-Bench subset with long contexts:
- A: Pi's compaction as it is; B: billion-context, pinned, updates and advisories off; C: the Jarvis rollover.
- Measured: pass rate, cost, pause seconds per compaction, prompt-cache hit rate.
- Rule: rank by pass rate; among candidates within 1 point of the best, pick the smallest in Jarvis code (A, then B, then C); equal size goes to the lower cost. 3.1 and 3.3 are built only if C wins.

## Reason
Not given.

## Consequences
- 0216's spike is this one. Installing billion-context for it asks him first.
- 3.4's other items (thresholds, overflow, what he sees) follow the winner.
- Amended by 0323: B chosen; the spike is not run.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
