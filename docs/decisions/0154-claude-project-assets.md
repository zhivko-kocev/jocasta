# 0154 Claude-format project assets

- Status: dropped
- Kind: product
- Phase/slice: 1/1.9
- Date: 2026-10-07

## Question
Does Jarvis load Claude-format project assets?

## Facts
Verified 2026-10-07: Pi 1.0.1 loads skills and prompts only from `<config dir>/skills|prompts` and the project's `.jarvis/skills|prompts` (`dist/core/resource-loader.js:751-762`), nothing under `.claude/`.
- Three of his projects carry Claude-format assets (10 skills, 6 agents, 3 rules; 8 skills and a hook; 3 skills; v4 §G).

## Options
| Option | Pros | Cons |
|---|---|---|
| Load them through `resources_discover` | Existing project skills and rules work without copies | Jarvis follows Claude's formats |
| Pi's defaults only | Nothing to build | The assets are invisible |

## Recommendation
Load them.

## Choice
Dropped (0231): no Claude Code formats in v1.

Originally:
Jarvis loads the project's `.claude/` assets through Pi's `resources_discover` event; which kinds is item 2.

## Reason
Not given.

## Consequences
- Agents go to 6.4 and hooks to 2.5.
- Reversed by 0231: no `.claude` assets in v1.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
