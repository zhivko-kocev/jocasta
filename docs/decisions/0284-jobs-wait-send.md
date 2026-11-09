# 0284 Jobs, wait and send

- Status: decided
- Kind: product
- Phase/slice: 6/6.3
- Date: 2026-10-08

## Question
Which tools run background work and wait for it?

## Facts
- Pi has no background bash. His omp usage: hub start 61, logs 56, jobs 42, stop 25; `wait` 106; hub wait 891, send 337 (appendix A).

## Options
| Option | Pros | Cons |
|---|---|---|
| `job`, `wait` and `send` | Matches his use | Three tools |
| `job` and `wait` only | Smaller | No messages to running subagents |

## Recommendation
`job`, `wait` and `send`.

## Choice
A `job` tool (start, logs, stop, list) for background commands; `wait` blocks until named jobs or subagents finish; `send` messages a running subagent. Output over 32 KiB spills to `artifact://` (0150).

## Reason
Not given.

## Consequences
- Slice 6.3 is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
