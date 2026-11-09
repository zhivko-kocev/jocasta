# 0241 Phase 3 slice status

- Status: decided
- Kind: product
- Phase/slice: 3/opening
- Date: 2026-10-08

## Question
Which phase 3 slices stay in v1 under lean Jarvis (0229)?

## Facts
Verified 2026-10-08 at phase 3's opening: Pi is 1.1.0; `@earendil-works/pi-durable` is 1.1.0 (npm); Pi's compaction defaults are a 16,384-token reserve and 20,000 kept tokens (`docs/settings.md:70-72`), v4's values.
- Usage (appendix A): todo 553 calls; `ask` 96 calls. Vision: V9 (smooth compaction, nothing lost), V10 and V11 (report-back).
- His tick-list kept 3.1 to 3.7; a follow-up resolved that 3.1 and 3.3 are built only if the Jarvis rollover wins 3.4's spike (0242).

## Options
| Option | Pros | Cons |
|---|---|---|
| 3.1 state doc | — | — |
| 3.2 todo | — | — |
| 3.3 briefing | — | — |
| 3.4 smooth compaction | — | — |
| 3.5 history search | — | — |
| 3.6 `ask` | — | — |
| 3.7 report-back | — | — |
| 3.8 titles and 3.9 close flow | — | — |

## Recommendation
3.2, 3.4, 3.5, 3.6 and 3.7 v1; 3.1 and 3.3 wait for 3.4; 3.8 and 3.9 after v1.

## Choice
- v1: 3.2 todo, 3.4 smooth compaction, 3.5 history search, 3.6 `ask`, 3.7 report-back.
- v1 only if 3.4's spike picks the Jarvis rollover (0242): 3.1 state doc and 3.3 briefing; otherwise after v1.
- After v1: 3.8 titles and 3.9 close flow (close signals and cleanup stay as 0119 and 0122 decided).

## Reason
Not given.

## Consequences
- Phase 3's exit smoke loses its state-doc wording unless 3.1 is built; it is rewritten when 3.4's agenda is grilled.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
