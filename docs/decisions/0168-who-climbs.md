# 0168 Who runs a climb

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
Who runs a hillclimb?

## Facts
- Claude Code ships `/claude-api hillclimb`, which reads the results layout of 0165; Jarvis gets subagents in 6.4.

## Options
| Option | Pros | Cons |
|---|---|---|
| Claude Code's `/claude-api hillclimb` | Works from phase 1; maintained by Anthropic | Depends on Claude Code |
| Jarvis itself, once subagents land (6.4) | Self-contained | No climbs until phase 6; Jarvis reimplements the loop |
| By hand | Full control | Slow; easy to overfit |

## Recommendation
Claude Code's `/claude-api hillclimb`.

## Choice
Now (0376): Jarvis itself; a scripted climb runs its proposer with the scripted-model test fixture.

Originally: Jarvis itself, once subagents land in 6.4.

## Reason
Not given.

## Consequences
- No climbs run before phase 6; 6.4's agenda gains the climb loop (train and test splits, one change per round, the noise check).
- Differs from the recommendation, which used Claude Code's skill from phase 1.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
