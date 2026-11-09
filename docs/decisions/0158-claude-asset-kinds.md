# 0158 Which Claude-format kinds load

- Status: dropped
- Kind: product
- Phase/slice: 1/1.9
- Date: 2026-10-07

## Question
Which `.claude/` kinds does Jarvis load?

## Facts
Verified 2026-10-07 (https://code.claude.com/docs/en/memory): rules without `paths:` load at launch and path-scoped ones on Read, Write or Edit; a project file can live at `./CLAUDE.md` or `./.claude/CLAUDE.md`.
- Skills load by name and description and expand on use; commands are prompt templates. Agents are 6.4's and hooks 2.5's.
- Asked as a multi-select; every kind was ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| `.claude/rules/*.md` | Unscoped rules at start; path-scoped on read (0156) | — |
| `.claude/skills/` | Indexed; bodies read on use | — |
| `.claude/commands/` | Slash commands in the TUI | — |
| `.claude/CLAUDE.md` | Claude Code's alternate project file | — |

## Recommendation
None was marked in the question.

## Choice
Dropped (0231).

Originally:
All four: `.claude/rules/*.md`, `.claude/skills/`, `.claude/commands/` and `.claude/CLAUDE.md`. `AGENTS.md` still wins over either `CLAUDE.md` in the same project (0155).

## Reason
Not given.

## Consequences
- `.claude/CLAUDE.md` counts as the project root's `CLAUDE.md` for 0155's rule.
- Reversed by 0231.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
