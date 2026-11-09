# 0227 Failing pre-tool hooks

- Status: dropped
- Kind: product
- Phase/slice: 2/2.5
- Date: 2026-10-08

## Question
What happens when a PreToolUse hook exits 1, crashes, can't start or times out?

## Facts
- Claude Code lets the tool run with a "hook error" notice; only exit 2 or a deny decision blocks. Its docs warn that "a mistyped path in settings.json leaves the gate silently disabled" (https://code.claude.com/docs/en/hooks, 2026-10-08).
- Principle 4: a guard fails closed. Lesson L28 raised this question.

## Options
| Option | Pros | Cons |
|---|---|---|
| Block | A broken policy hook can't silently switch itself off | Ported hooks that exit 1 by accident start blocking |
| Proceed with a notice (Claude Code) | Ported hooks behave the same | A broken gate is silently off |

## Recommendation
Block.

## Choice
Dropped (0231).

Originally:
Any failure of a PreToolUse hook (a non-zero exit other than a valid decision, a crash, a missing script or a timeout) blocks the call with the hook's error.

## Reason
Not given.

## Consequences
- Differs from Claude Code; the denial text says the hook failed, with its stderr's first line.
- Reversed by 0231.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
