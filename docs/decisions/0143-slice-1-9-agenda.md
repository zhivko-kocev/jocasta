# 0143 Slice 1.9 agenda

- Status: decided
- Kind: process
- Phase/slice: 1/1.9
- Date: 2026-10-07

## Question
Which decisions does slice 1.9 (context loading) grill, now that v4 P1.6-1 to P1.6-7 are decided fresh (0088)?

## Facts
Verified 2026-10-07: Pi 1.0.1 loads one context file per directory, the first of `AGENTS.override.md`, `AGENTS.md`, `AGENTS.MD`, `CLAUDE.md`, `CLAUDE.MD` (`dist/core/resource-loader.js:116`), so `CLAUDE.md` is read only where no `AGENTS.md` exists. This answers the plan's "verified at the slice".
- Lessons L19 and L51 map to 1.9; the nested AGENTS.md row (0083) names 1.9.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of seven: load Claude-format assets at all; which kinds; `CLAUDE.md` beside `AGENTS.md`; path-scoped rules and nested AGENTS.md on read; user-level `~/.claude/*`; `@imports`; command arguments | Each asset kind decided on its own; agents go to 6.4 and hooks to 2.5 | Seven questions |
| The plan's three | Fewer | The overlap and the nested-rules row stay open |

## Recommendation
The fresh agenda of seven.

## Choice
The fresh agenda of seven.

## Reason
Not given.

## Consequences
- Plan 1.9's list becomes the seven items, citing this record.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
