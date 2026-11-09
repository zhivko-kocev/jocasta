# 0180 Workspace tree in the prompt

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
Does the prompt carry a file tree of the workspace?

## Facts
Verified 2026-10-07: Pi 1.0.1 puts the working directory in the prompt, not a file tree (`packages/coding-agent/src/core/system-prompt.ts`).
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Never | No stale tree; smaller cached prefix | The model lists files itself |
| On request | — | — |

## Recommendation
Never.

## Choice
Never.

## Reason
Not given.

## Consequences
- Appendix G's lever is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
