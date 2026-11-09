# 0226 Hook events

- Status: dropped
- Kind: product
- Phase/slice: 2/2.5
- Date: 2026-10-08

## Question
Which Claude Code hook events does Jarvis fire?

## Facts
- Claude Code's events and exit-2 effects: PreToolUse blocks the call, PostToolUse shows stderr to the model, UserPromptSubmit blocks the prompt, Stop keeps the agent working, SessionStart and SessionEnd show stderr to the user, PreCompact blocks compaction (https://code.claude.com/docs/en/hooks, 2026-10-08).
- Pi 1.1.0's matching events are listed in 0225.

## Options
| Option | Pros | Cons |
|---|---|---|
| PreToolUse → `tool_call` | — | — |
| PostToolUse → `tool_result` | — | — |
| UserPromptSubmit → `input` | — | — |
| Stop → `agent_before_settle` | — | — |
| SessionStart and SessionEnd → `session_start` and `session_shutdown` | — | — |
| PreCompact → `session_before_compact` | — | — |

## Recommendation
None was marked in the question.

## Choice
Dropped (0231).

Originally:
All six: PreToolUse, PostToolUse, UserPromptSubmit, Stop, SessionStart and SessionEnd, PreCompact, each on the Pi event named.

## Reason
Not given.

## Consequences
- PreCompact's interaction with smooth compaction is checked in 3.4.
- PostToolUse runs on `tool_result`, where Pi skips a throwing handler (0078); the hook runner catches its own errors.
- Reversed by 0231.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
