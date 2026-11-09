# 0228 Hook timeouts and shell

- Status: dropped
- Kind: product
- Phase/slice: 2/2.5
- Date: 2026-10-08

## Question
How long may a hook run, which shell runs it, and how is it stopped?

## Facts
- Claude Code: default timeout 600 s for command hooks and 30 s for UserPromptSubmit; the `shell` field picks `bash` or `powershell`, defaulting to Git Bash, or PowerShell on Windows without Git Bash (https://code.claude.com/docs/en/hooks, 2026-10-08).
- Lesson L28: on Windows, killing `bash -c` leaves its children running and the stdout read pending.

## Options
| Option | Pros | Cons |
|---|---|---|
| Claude Code's defaults and shell rules; a timeout kills the process tree and stops waiting on the pipes | Ported hooks behave the same | A stuck hook can stall a turn for 10 minutes |
| The same with a 60 s default | Faster failure | Differs from Claude Code |

## Recommendation
As recommended.

## Choice
Dropped (0231).

Originally:
Claude Code's timeouts (600 s, 30 s for UserPromptSubmit, per-hook `timeout`) and `shell` rules; on timeout Jarvis kills the hook's whole process tree and stops reading its pipes.

## Reason
Not given.

## Consequences
- With 0227, a timed-out PreToolUse hook blocks the call.
- Reversed by 0231.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
