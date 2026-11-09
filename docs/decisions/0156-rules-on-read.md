# 0156 Path-scoped rules and nested AGENTS.md on read

- Status: decided
- Kind: product
- Phase/slice: 1/1.9
- Date: 2026-10-07

## Question
Is the catalog's "path-scoped rules and nested AGENTS.md loaded on read" row (0083) in v1?

## Facts
- Claude Code loads `.claude/rules/*.md` with `paths:` globs when a matching file is read; opencode v1.18.22 attaches the AGENTS.md files above a read file once per message (`packages/opencode/src/session/instruction.ts:179-218`); Pi loads AGENTS.md only from the working dir's ancestors.
- Lesson L19 attaches each rule once per conversation, after the tool result, recomputed from the transcript so the cache prefix stays stable.
- Usage: 3 rules in one project.
- No recommendation was marked in the question.

## Options
| Option | Pros | Cons |
|---|---|---|
| v1 | Rules apply when relevant; cheap until triggered | Context grows mid-conversation |
| After v1 | Smaller 1.9 | Path rules load always or never |
| Dropped | Simplest | The same, for good |

## Recommendation
None was marked in the question.

## Choice
Now (0232): nested `AGENTS.md` only; path-scoped rules are dropped.

Originally:
v1. When a tool reads a file, Jarvis attaches the `.claude/rules` whose `paths:` match it and the AGENTS.md files between the file and the project root that aren't loaded yet, once per thread window, after the tool result.

## Reason
Not given.

## Consequences
- Appendix A's row is v1.
- Amended by 0232: nested `AGENTS.md` only; path-scoped rules dropped.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
