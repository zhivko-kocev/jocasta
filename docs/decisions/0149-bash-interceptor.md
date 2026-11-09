# 0149 Bash interceptor

- Status: decided
- Kind: product
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
Does Jarvis block shell commands that duplicate a tool?

## Facts
- His omp history has shell calls the interceptor would redirect: grep 1,021, sed 325, ls 252, cat 221, find 36, head 35 (appendix A).
- omp v18.6.0 ships it off by default (`packages/coding-agent/src/exec/settings.ts:134-137`); it skips piped stages and redirects only to active tools (`src/tools/bash-interceptor.ts:97-132`).
- Lesson L69 blocked `cat` and `… && grep` on Pi 1.0.1.
- Appendix G recommends on.

## Options
| Option | Pros | Cons |
|---|---|---|
| On, with omp-style rules | Smaller structured output; reads count for edit-requires-read | A refused shell idiom costs a turn |
| Off | Never in the way | Shell reads bypass the read stamp and the spill |

## Recommendation
On.

## Choice
On: `cat`, `head`, `tail` → `read`; `grep`, `rg` → `grep`; `find` → `find`; `ls` → `ls`; `sed -i` → `edit`. A blocked call's reason names the tool. Piped stages and commands whose tool isn't active are never blocked.

## Reason
Not given.

## Consequences
- Message, as previewed: `blocked: Use the read tool instead (it shows line tags and counts as a read).`
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
