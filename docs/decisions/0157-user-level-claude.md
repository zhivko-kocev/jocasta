# 0157 User-level `~/.claude`

- Status: decided
- Kind: product
- Phase/slice: 1/1.9
- Date: 2026-10-07

## Question
Does Jarvis load user-level `~/.claude/*`?

## Facts
- His user level has 4 agents, 2 skills and a PreToolUse git-guard hook (v4 §G).
- Jarvis's user-level files live in `~/.jarvis/agent` (0066–0069). Principle 3: one owner per setting.

## Options
| Option | Pros | Cons |
|---|---|---|
| Never | One owner for user-level behavior | Shared skills must be copied or linked |
| Also load `~/.claude/*` | One personal set | Two owners; Claude-specific text reaches Jarvis |

## Recommendation
Never.

## Choice
Never: Jarvis's user level is `~/.jarvis/agent` only.

## Reason
Not given.

## Consequences
- None.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
