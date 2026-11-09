# 0193 Slice 2.5 agenda

- Status: decided
- Kind: process
- Phase/slice: 2/2.5
- Date: 2026-10-08

## Question
Which decisions does slice 2.5 (Claude-format hooks) grill, now that v4 2.5's decisions 1–13 are decided fresh (0088)?

## Facts
- He has 2 user PreToolUse hooks and 1 project PostToolUse hook in Claude Code (appendix A).
- Lesson L28, checked against https://code.claude.com/docs/en/hooks: in Claude Code a hook that exits 1 or times out doesn't block; exit 2 or a block decision does. Principle 4: fail closed on anything that guards.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda: status; events; where hooks load from; exit codes and JSON output; exit 1 or timeout in a pre-tool hook; timeouts and Windows process trees; env vars; order against the guard; matchers | Status first (principle 8) | Nine questions |
| Change it | — | — |

## Recommendation
The fresh agenda.

## Choice
The fresh agenda.

## Reason
Not given.

## Consequences
- Plan 2.5's text becomes the agenda, citing this record.
- Superseded by 0231: slice 2.5 is dropped.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
