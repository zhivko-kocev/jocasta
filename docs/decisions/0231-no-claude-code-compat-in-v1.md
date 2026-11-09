# 0231 No Claude Code compatibility in v1

- Status: decided
- Kind: product
- Phase/slice: 1/1.9, 2/2.1, 2.3, 2.5, 6/6.4
- Date: 2026-10-08

## Question
Does v1 support Claude Code's formats?

## Facts
- His words: "i dont want dependencies to claude code" and "maybe it is better for v1 we dont support claude code stuff we need our own barebone PI with our extensions or extensions from other devs but that fit our idea. and javis handles the other stuff".
- Decided items that exist only for Claude Code compatibility: 0154, 0158, 0159, 0160 (`.claude` rules, skills, commands and `.claude/CLAUDE.md`, with Claude's `@imports` and argument rules); 0225–0228 (Claude-format hooks); 0220's `.claude` hooks and `.mcp.json`; 0208's Claude tool names.
- Verified 2026-10-08 in Pi 1.1.0: Pi loads `AGENTS.md` and `CLAUDE.md` as context files (`dist/core/resource-loader.js`), skills from `.agents/skills`, prompt templates from `.jarvis/prompts` and MCP servers from `.jarvis/mcp.json` (`docs/security.md:37-53`); extensions see `tool_call`, `tool_result`, `input`, `agent_before_settle`, `session_start`, `session_shutdown` and `session_before_compact` (`dist/core/extensions/types.d.ts:1165-1203`); Pi packages from other developers load the same way (0045).
- Lean Jarvis (0229).

## Options
| Option | Pros | Cons |
|---|---|---|
| Drop all Claude Code compatibility from v1; hooks are Pi extensions; rules use Jarvis's and Pi's tool names | Bare Pi plus extensions; no second format | His `.claude` assets and Claude rules need porting |
| The same, keeping Claude tool names in rules | His Claude deny rules copy over | Claude names stay |
| Decide in the pruning pass | — | v1 keeps growing |

## Recommendation
Drop all of it.

## Choice
No Claude Code formats in v1. Pi's own behavior stays: it loads `CLAUDE.md` beside `AGENTS.md` (0155) and `.agents/skills`.
- Reversed: 0154, 0158, 0159, 0160 (no `.claude` assets), 0225–0228 (no Claude-format hooks; a hook is a Pi extension, his or another developer's).
- Amended: 0208 keeps `Tool(specifier)` strings with Jarvis's and Pi's tool names (`bash(git push *)`, `read(**/.env*)`); 0220 gates Pi's list only; 0157 stands.
- Slice 2.5 is dropped; its answers about hook output, order and matchers lapse.

## Reason
"i dont want dependencies to claude code"; "maybe it is better for v1 we dont support claude code stuff we need our own barebone PI with our extensions or extensions from other devs but that fit our idea. and javis handles the other stuff"

## Consequences
- His two user PreToolUse hooks and one project PostToolUse hook become Pi extensions or `permissions.yaml` rules; his git-guard hook is covered by 2.2.
- 6.4's agent definitions come from Jarvis's config dir and bundled agents only.
- Appendix I drops `JARVIS_PROJECT_DIR` and `CLAUDE_PROJECT_DIR`.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
