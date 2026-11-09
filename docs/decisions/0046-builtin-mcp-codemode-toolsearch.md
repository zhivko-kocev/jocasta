# 0046 Built-in MCP, Codemode and tool_search

- Status: v1
- Kind: feature
- Phase/slice: 0/0.2
- Vision: V8
- Date: 2026-10-04

## Question
Do Pi's built-in extensions load?

## Facts
Pi `docs/settings.md` (`v1.0.1`): the built-ins are `builtin:mcp`, `builtin:llama.cpp`, `builtin:codemode` and `builtin:tool-search`; they load by default, and `-builtin:<name>` in settings disables one. Per-server exposure is decided in 6.7.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep all | MCP from day one | More tools in the prompt |
| Keep MCP, Codemode and tool_search; drop llama.cpp | Drops an unused local provider | One delta setting |
| Disable all | Smallest prompt | No MCP until 6.7 |

## Recommendation
Keep all (principle 1); llama.cpp is decided with providers in 0.4.

## Choice
Keep all four built-ins.

## Reason
Not given.

## Consequences
No settings delta. 0.4 can still drop llama.cpp as a provider.
