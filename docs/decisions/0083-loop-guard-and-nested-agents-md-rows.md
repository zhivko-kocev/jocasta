# 0083 Loop-guard and nested AGENTS.md rows

- Status: decided
- Kind: product
- Phase/slice: 0/appendix A
- Vision: V8
- Date: 2026-10-07

## Question
Do two existing catalog rows take the findings from the learn-harnesses clones?

## Facts
Verified 2026-10-07 in the pinned clones in `D:/learn-harnesses/repos`.
- mini-swe-agent v2.4.6 ends a run after three malformed calls in a row (`max_consecutive_format_errors = 3`, `src/minisweagent/agents/default.py:32-33`). The loop-guard row covered identical calls only.
- opencode v1.18.22 walks up from a file being read and attaches the AGENTS.md files above it, once per message (`packages/opencode/src/session/instruction.ts:179-218`). Pi loads AGENTS.md from the working dir's ancestors only. The row covered Claude Code's path-scoped rules only.

## Options
| Option | Pros | Cons |
|---|---|---|
| Both | 1.8 and 1.9 open with the whole design space | Two more variants to grill |
| Loop-guard row only | The malformed-call stop reaches 1.8 | Nested AGENTS.md in monorepos stays unseen at 1.9 |
| AGENTS.md row only | 1.9 sees nested AGENTS.md | 1.8 misses the format-error stop |
| Neither | — | Both stay out |

## Recommendation
Both.

## Choice
Both.

## Reason
Not given.

## Consequences
- Appendix A's loop-guard row (1.8) adds mini-swe-agent; the path-scoped rules row (1.9) becomes "path-scoped rules and nested AGENTS.md loaded on read" and adds opencode.
