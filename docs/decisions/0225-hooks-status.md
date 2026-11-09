# 0225 Claude-format hooks status

- Status: dropped
- Kind: product
- Phase/slice: 2/2.5
- Date: 2026-10-08

## Question
Are Claude-format hooks in v1?

## Facts
Verified 2026-10-08: Claude Code's hook format is documented at https://code.claude.com/docs/en/hooks. Pi 1.1.0 has matching events: `tool_call`, `tool_result`, `input`, `agent_before_settle`, `session_start`, `session_shutdown`, `session_before_compact` (`dist/core/extensions/types.d.ts:1165-1203`).
- He has 2 user PreToolUse hooks and 1 project PostToolUse hook in Claude Code (appendix A). Jarvis loads nothing from `~/.claude` (0157); project `.claude` hooks need trust (0220).

## Options
| Option | Pros | Cons |
|---|---|---|
| v1 | His hooks keep working | A hook runner to build |
| After v1 | — | His hooks are ported to rules or extensions |
| Dropped | — | — |

## Recommendation
v1.

## Choice
Dropped (0231): a hook is a Pi extension.

Originally:
v1: Claude-format command hooks from the project's `.claude/settings.json` (trusted projects) and from `~/.jarvis/agent/settings.json`.

## Reason
Not given.

## Consequences
- His two user hooks move from `~/.claude/settings.json` into `~/.jarvis/agent/settings.json` by hand.
- Only `command` hooks; `http`, `prompt`, `agent` and `mcp_tool` hooks aren't supported.
- Appendix A's hooks row is v1.
- Reversed by 0231: no Claude-format hooks in v1.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
