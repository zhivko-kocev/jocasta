# 0155 AGENTS.md beside CLAUDE.md

- Status: decided
- Kind: product
- Phase/slice: 1/1.9
- Date: 2026-10-07

## Question
Which file loads when a folder has both `AGENTS.md` and `CLAUDE.md`?

## Facts
Verified 2026-10-07: Pi 1.0.1 reads only the first of `AGENTS.override.md`, `AGENTS.md`, `AGENTS.MD`, `CLAUDE.md`, `CLAUDE.MD` per directory (`dist/core/resource-loader.js:116`).
- Lesson L19 reports that Claude Code does the reverse; copies on Windows can differ only by line endings.

## Options
| Option | Pros | Cons |
|---|---|---|
| Both, deduped by normalized content | Nothing written for Claude Code is lost | Two files can contradict; more context |
| Pi's order: `AGENTS.md` wins | Nothing to build | A `CLAUDE.md` beside an `AGENTS.md` is ignored |
| Claude's order | Matches Claude Code | Fights Pi's loader |

## Recommendation
Both, deduped.

## Choice
Pi's order: per folder, `AGENTS.md` wins and `CLAUDE.md` loads only where no `AGENTS.md` exists.

## Reason
Not given.

## Consequences
- Nothing to build for this item.
- Differs from the recommendation, which loaded both.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
