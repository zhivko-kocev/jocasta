# 0152 Loop guard

- Status: decided
- Kind: product
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
When does Jarvis stop a looping run?

## Facts
Verified 2026-10-07.
- Pi 1.0.1 has no loop guard.
- Appendix G (omp plan): five identical calls in a row stop the run.
- opencode v1.18.22 asks permission after three identical calls (`packages/opencode/src/session/processor.ts:29,356-376`); mini-swe-agent v2.4.6 ends a run after three malformed calls in a row (`src/minisweagent/agents/default.py:32-33`).
- His omp history: the guard fired once. Headless runs can't ask until 3.6 (0141).

## Options
| Option | Pros | Cons |
|---|---|---|
| Stop at 5 identical calls in a row and at 3 malformed calls in a row | Mechanical; the same headless and by hand | By hand, he sends a message to continue |
| Ask at 3 identical; stop at 3 malformed | Catches loops sooner | Headless needs 3.6; more interruptions |
| Identical calls only, stop at 5 | One rule | Broken calls burn turns until the run budget |

## Recommendation
Stop at 5 identical and at 3 malformed.

## Choice
The turn stops after five identical tool calls in a row (same tool, same arguments) or three malformed calls in a row (arguments that fail the tool's schema), naming the reason. A headless run exits 1.

## Reason
Not given.

## Consequences
- Appendix G's loop-guard lever is decided; appendix A's loop-guard row is v1.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
