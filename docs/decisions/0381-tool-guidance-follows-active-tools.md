# 0381 Tool guidance follows the active tools

- Status: decided
- Kind: product
- Phase/slice: 1/1.11
- Date: 2026-10-10

## Question
Jarvis's prompt section says to use the read, grep, find and ls tools while Pi's default active tools are read, bash, edit and write, and Pi's own prompt then says to use bash for ls, rg and find. Which wins?

## Facts
Verified 2026-10-10.
- Pi's default active tools are read, bash, edit and write (`dist/core/agent-session.d.ts:130`).
- With bash active and grep, find and ls inactive, Pi adds "Use bash for file operations like ls, rg, find" (`dist/core/system-prompt.js:46-55`).
- Jarvis's section says "- Use the read, grep, find and ls tools, not shell equivalents." (`src/harness/prompt/prompt.ts`).
- The bash interceptor blocks a command only when the tool it points to is active.

## Options
| Option | Pros | Cons |
|---|---|---|
| The line names only the active ones of read, grep, find and ls | The prompt never names a tool it lacks; agrees with Pi's line and the interceptor | The line changes with the tool set |
| Keep the fixed line | No change | Contradicts Pi's line |

## Recommendation
Follow the active tools.

## Choice
The line is built from the active tools: the active ones of read, grep, find and ls, in that order, render "- Use the <list> tool[s], not shell equivalents." with the list joined by ", " and a final " and ". With none active the line is omitted.

## Reason
The prompt must not contradict itself.

## Consequences
- Amends 0172.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
