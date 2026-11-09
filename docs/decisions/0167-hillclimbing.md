# 0167 Hillclimbing

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
Does Jarvis hillclimb its own surfaces?

## Facts
Verified 2026-10-07 (0079): climb cheap, attributable surfaces one change per round; the analyzer reads a train split and never the test split, stratified by tag; a change stays only when both rise beyond noise; failures never go into the prompt. The post's cost climb found a smaller model at low effort matching a larger one at about half the cost.
- Real-provider runs bill extra usage (0062).

## Options
| Option | Pros | Cons |
|---|---|---|
| Yes, by R15's rules | Defaults earn their place; cheaper roles found by measurement | Each climb costs real-provider runs |
| No | No climb runs | Prompts and role settings stay guesses |

## Recommendation
Yes.

## Choice
Yes, by R15's rules, on Jarvis's shipped prompt section, tool descriptions, briefing, boss manual, bundled agents, and a role's model and effort.

## Reason
Not given.

## Consequences
- Who runs a climb is 0168.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
