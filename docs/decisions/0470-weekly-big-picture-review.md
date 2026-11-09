# 0470 A weekly big-picture review per active project

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
Each thread is reviewed on its own diff. Does Jarvis also review what was merged across threads, to catch design drift?

## Facts
Verified 2026-10-23.
- DHH's "Swiss cheese": 20–30 PRs that were each "individually reasonable" left Basecamp 5's architecture drifting (Rails World 2026 keynote, 00:56:52, https://www.youtube.com/watch?v=vDjW_dRyKXY, read through the transcript at https://gist.github.com/cristoslc/d17809f90a256af4d1008a1a36f9abaa; `local://omarchy-OmarchyAI.md`).
- Jarvis reviews each thread's own diff only (0394, 0425).

## Options
| Option | Pros | Cons |
|---|---|---|
| On his word | No cost unless he asks | Drift goes unseen until he thinks to ask |
| Weekly, automatic | Drift is caught every week without him asking | Tokens every week; false positives |
| No | No cost | Drift across threads is never reviewed |

## Recommendation
On his word.

## Choice
Once a week, for each project with merges in the past 7 days, Jarvis runs one read-only review of everything merged that week (main a week ago → main now) against the project's rules and lessons, looking for design drift across changes; the result reaches the boss as an event and the dashboard. He can also ask the boss for one any time. Day and hour are the builder's default (outside quiet hours); the reviewer follows 0460.

## Reason
"Weekly, automatically".

## Consequences
- Differs from the recommendation.
- The day, hour, event text and dashboard line are chosen by the builder and listed as built.
