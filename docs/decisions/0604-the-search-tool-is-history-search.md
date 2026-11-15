# 0604 The search tool is named `history_search`

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-14

## Question
With the graph gone (0603), does the search tool keep the name `graph_search`?

## Facts
- The tool is listed to every omp pane by `jocasta mcp` and named in the boss manual, the README, `jocasta --help` and the plan.
- After 0603 it searches the stored summaries of past sessions and the threads' reported decisions; there is no graph behind it.
- A tool's name and description are part of every pane's fixed prompt and are what the model chooses by.

## Options
| Option | Pros | Cons |
|---|---|---|
| `history_search` | Says what it searches | A rename in the MCP server, the boss manual, the help text and the plan |
| Keep `graph_search` | No churn | The name promises a graph that is gone |

## Recommendation
`history_search`.

## Choice
`history_search`, for panes and for the boss. The old name is not kept as an alias.

## Reason
His answer: "history_search".

## Consequences
- Renamed wherever the tool is listed, called or described, the benchmark's layer and H4 included; built with 0603.
- Amends the tool name in 0389, 0484, 0542 and 0562.
