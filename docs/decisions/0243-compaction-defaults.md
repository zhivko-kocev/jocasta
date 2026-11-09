# 0243 Compaction thresholds, overflow and marker

- Status: decided
- Kind: product
- Phase/slice: 3/3.4
- Date: 2026-10-08

## Question
Which thresholds, overflow handling and visible marker does compaction use?

## Facts
Verified 2026-10-08 in Pi 1.1.0: reserve 16,384 and keep 20,000 recent tokens by default (`docs/settings.md:70-72`), v4's values; a provider context-overflow error or an early `length` stop triggers one compact-and-retry (`docs/compaction.md:39`).
- Asked as a tick-list (0089); all ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Thresholds: Pi's defaults | No Jarvis code | — |
| Overflow: Pi's one compact-and-retry | No Jarvis code | — |
| What he sees: the winner's own marker | No Jarvis code unless C wins | — |

## Recommendation
All three.

## Choice
Pi's default thresholds (billion-context's own nudges if it wins), Pi's one compact-and-retry on overflow, and the winning mechanism's own transcript marker (a one-line Jarvis marker only if the Jarvis rollover wins).

## Reason
Not given.

## Consequences
- Slice 3.4 is decided.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
