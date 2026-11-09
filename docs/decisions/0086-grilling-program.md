# 0086 Grilling program

- Status: decided
- Kind: process
- Phase/slice: 0/section 4
- Vision: V8, V14
- Date: 2026-10-07

## Question
When are the decisions of phases 1–9 made?

## Facts
- 0001 grilled each slice right before it was built; 0004 re-asks planning choices in their slice; 0008 decides each feature in its slice. V8 reads "At implementation time he decides every feature that goes in or stays out."
- Phases 1–9 list roughly 300 decisions, about 70 undecided appendix A rows and about 25 appendix G levers; many slices carry over v4 designs (0088).
- Facts age between a grilling and the build; 0077 kept fact notes out of slices for that reason.
- Some decisions need a spike or a build-time measurement: 4.2's Windows spike of herdr and in-process tabs, 5.0's memory spike over threads recorded in phases 1–4, and 6.5's test of how Pi aborts and retries mid-stream. herdr is not installed, and installs need his approval.

## Options
| Option | Pros | Cons |
|---|---|---|
| Two passes, then build: scope for every slice and catalog row first, then design for the v1 slices | Scope cuts first; design questions only for what is in v1 | Two rounds over the plan |
| One pass in phase order, then build | Simplest to run | Design questions for features later dropped |
| Upfront through phase 4 | First build sooner | Not everything |
| Per slice, as before | No change | Not what he asked |

Follow-up on the chosen option, with its own options (both | status first only | spike rules only | neither); recommended and chosen: both.

## Recommendation
Two passes, then build.

## Choice
One pass in phase order: every decision of every slice in phases 1–9, then the build. Two rules keep the pass from stalling:
- A slice or appendix A row that adds a feature opens with its status: v1, after v1 or dropped (principle 8, 0016). Its design questions follow only if it is v1.
- A decision that needs a spike or a build-time measurement is recorded as a rule: what is measured, and which option each result picks. The spike runs when the slice is built; a result the rule doesn't settle reopens the decision.

## Reason
"i want to decide everything upfrontn now we have good resources sources links repos, i want minimalistic but still with the best features out of the harnesses custom made tailored for me. we can start with grilling sessions until we get full picture for the harness then we build"

## Consequences
- Plan section 4: the grilling program replaces per-slice grilling; the build starts when the pass ends and runs in phase order, with a review at each phase end (0003).
- Phase openings keep their fact check; a decided item whose facts changed is asked again, saying what changed (0004).
- The vision image is unchanged; V8's "at implementation time" now means before the build, by his choice here.
- The pass starts at slice 1.1. AGENTS.md's "Where we are" names the next slice whenever a session stops.
- Differs from the recommendation, which was two passes.
