# 0575 The follow-up drops f-keepalive; A6's layer builds its code index before the trial's clock

- Status: v1
- Kind: benchmark
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
The review found that the keep-alive arm can't differ from A6 in the task harness, and that A6's first relationship `graph_search` builds the code index inside the trial (and inside omp's MCP timeout). Keep the arm? Where does the index build go?

## Facts
Verified 2026-11-09 by the review (0573), report EVL (EVL-9) and report SET (SET-1).
- omp's cache warmer refreshes "idle" entries only between agent runs; in A6 the plan review is answered inside the event (`evals/plan-review.ts:68`) and omp stops at the execution's final `agent_end` (:86), so a trial has no idle gap for a ping.
- Each review turn is its own print-mode process, so `f-keepalive` (`providers.cacheWarming: idle`) runs the same as A6's `streaming`.
- A first full index took 18–34 s on 42–91 MB of source (0562); `INDEX_MS = 600_000` (`code-index.ts:57-58`); each A6 trial starts with a fresh `<layer>/data`, so every trial's index is cold (`src/frontier/layer/bench.ts:84-89`).
- The agent then sees the code-index tool fail on first use (omp's 30 s MCP default), which biases A6 and `f-index-off` against the index.

## Options
| Item | Options (pros / cons) |
|---|---|
| EVL-9 keep-alive arm | (a) drop `f-keepalive` (four comparisons, 100 trials) and report pings as not measured by the task run (honest, 20 trials fewer / 0541's K9 stays open) · (b) a fixed 6-minute "reading time" before the plan answer in every A6-family arm (the product's real wait / +6 min per trial) · (c) keep it as exploratory (no change / a verdict from noise) |
| SET-1 C index build | Build the index in the trial's setup, before its clock, and record the build time apart (removes the cold case / index time leaves the trial, a methodology call) · Keep it inside the trial (as the product's first use / the 30 s timeout and a biased tool) |

## Recommendation
EVL-9 (a), with pings measured where waits are real (H8); SET-1 C for the bench, beside A's timeouts (0577).

## Choice
- EVL-9: `f-keepalive` leaves the follow-up study, which keeps four comparisons (100 trials). Keep-alive pings are reported as not measured by the task run (no idle gap in a trial) and are measured where the waits are real: H8's threads waiting on his Allow. The dry run's list reads `not compared: pitchfork (0560), keep-alive pings (no idle gap in a trial)`.
- SET-1 C (methodology): each A6-family trial's layer builds the code index during setup, before the trial's clock; the row records the build time apart (e.g. `"layer": {"indexBuildMs": 21840}`), and wall time starts after it.

## Reason
"(a) Drop `f-keepalive`"; SET-1 "C", because in his daily use his projects' index is already built.

## Consequences
- Amends 0543 and 0545 (the follow-up's arms: four comparisons), 0567 (its "pings tested only in their arm": that arm is gone), 0571 (the follow-up's five comparisons become four) and 0541 (K9 stays open for the task run; H8 measures pings).
- The `f-keepalive` variant and its dry-run line go; result rows of the A6 family gain `layer.indexBuildMs`.
- Amended by 0586 (2026-11-09): H8 doesn't measure pings either; on his subscription omp's 1-hour cache entries are never refreshed while idle, and elsewhere omp pings only when a ping should save at least $0.05.
