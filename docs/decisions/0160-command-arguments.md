# 0160 Command arguments

- Status: dropped
- Kind: product
- Phase/slice: 1/1.9
- Date: 2026-10-07

## Question
How are arguments substituted in loaded commands and skills?

## Facts
Verified 2026-10-07.
- Claude Code: `$ARGUMENTS`, `$ARGUMENTS[N]` and `$N` are 0-based, so `$0` is the first argument (https://code.claude.com/docs/en/skills).
- Pi 1.0.1: `$1` is the first argument; `$@` and `$ARGUMENTS` are all of them; `${N:-default}` sets a default (`dist/core/prompt-templates.js:46-55`).
- A Claude command using `$0` or `$1` run through Pi's substitution gets the wrong arguments.

## Options
| Option | Pros | Cons |
|---|---|---|
| Claude Code's semantics for files from `.claude/`, Pi's for Pi's own templates | Commands behave as in Claude Code | Two rules, chosen by the file's origin |
| Pi's semantics everywhere | One rule | Every `$0` and `$1` shifts by one |

## Recommendation
Claude Code's semantics for `.claude/` files.

## Choice
Dropped (0231); Pi's own substitution applies to Pi's templates.

Originally:
Commands and skills loaded from `.claude/` use Claude Code's substitution (0-based `$N`, `$ARGUMENTS[N]`, `$ARGUMENTS`, and `ARGUMENTS: <input>` appended when nothing receives it); Pi's own templates keep Pi's.

## Reason
Not given.

## Consequences
- None.
- Reversed by 0231.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
