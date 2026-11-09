# 0179 Speculative tool execution

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
Do tools run before the model's message completes?

## Facts
Verified 2026-10-07: Pi 1.0.1 never runs tools before the message completes; each tool is sequential or parallel, and results are emitted in source order (`packages/agent/src/types.ts:42-45`).
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Off | Side effects keep their order | — |
| On | Lower latency | Reorders side effects |

## Recommendation
Off.

## Choice
Off: Pi's behavior is kept and not optimized later.

## Reason
Not given.

## Consequences
- Appendix G's lever is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
