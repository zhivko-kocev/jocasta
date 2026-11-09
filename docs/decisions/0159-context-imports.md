# 0159 `@path` imports

- Status: dropped
- Kind: product
- Phase/slice: 1/1.9
- Date: 2026-10-07

## Question
Does Jarvis expand `@path` imports in context files?

## Facts
Verified 2026-10-07 (https://code.claude.com/docs/en/memory): Claude Code expands `@path` imports at launch, relative to the importing file, up to four hops; code spans and fenced blocks are skipped; imports that resolve outside the project need a one-time approval.
- Pi 1.0.1 doesn't expand them.

## Options
| Option | Pros | Cons |
|---|---|---|
| Claude Code's rules | Files that import shared docs work as written | Imported text costs context at every start |
| Pi's behavior (none) | Nothing to build | `@docs/x.md` stays literal |

## Recommendation
Claude Code's rules.

## Choice
Dropped (0231).

Originally:
Jarvis expands `@path` imports in `AGENTS.md` and `CLAUDE.md` files: relative to the importing file, up to four hops, skipping code spans and fenced blocks. An import outside the project is refused until phase 2's approvals can ask for it.

## Reason
Not given.

## Consequences
- Phase 2's approval interface (appendix I) gains outside-project imports.
- Reversed by 0231.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
