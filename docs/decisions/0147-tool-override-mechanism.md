# 0147 Tool override mechanism

- Status: decided
- Kind: product
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
How does Jarvis change tool behavior?

## Facts
Verified 2026-10-07 in Pi 1.0.1: `tool_call` handlers can block with a reason or rewrite the input; `tool_result` handlers chain and can replace content (replacing `content` must also replace `structuredContent`); an extension can re-register a built-in tool by name (`examples/extensions/ssh.ts`).
- Lesson L69 built the read guard, the interceptor and the spill as hooks only.
- Hashline (0146) changes the edit schema, which hooks can't do.

## Options
| Option | Pros | Cons |
|---|---|---|
| Hooks for guards; replace a built-in only when its schema changes | Pi's tools stay Pi's where possible; guards stay separate | Two techniques |
| Hooks only | One technique | Rules out hashline |
| Replace every touched tool | One place per tool | Jarvis owns copies of Pi's tools |

## Recommendation
Hooks; replace only when the schema changes.

## Choice
Guards (read stamps, interceptor, spill, loop guard) are `tool_call` and `tool_result` hooks. A built-in is replaced by name only when its schema changes: today `read` and `edit` (0146).

## Reason
Not given.

## Consequences
- Replacing a tool by name is a documented seam (plan 5.4's "Replacing tools" row), so plan 5.5's list of non-documented seams is unchanged; a Pi bump still re-runs the read and edit tests.
- Appendix I's handler order (guard, then redaction, then the rest) applies to these hooks.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
