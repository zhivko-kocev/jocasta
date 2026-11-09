# 0342 A personal graph from confirmed facts and generalized decisions

- Status: decided
- Kind: product
- Phase/slice: 5/5.2, 5.9 (amends 0280)
- Date: 2026-10-08

## Question
Does Jarvis keep a graph about him across projects, and what feeds it?

## Facts
- 0280: "If the Graphiti spike keeps the graph, the log also feeds a personal graph of his decisions." The decision log is sealed per project.
- 0277: nothing from one project reaches another; the personal store holds only confirmed entries, never project content.
- The graph is in v1 (0334); its group ids are per project.
- His raw decisions name project and client details.

## Options
| Option | Pros | Cons |
|---|---|---|
| Confirmed personal facts plus generalized decisions he confirms | A graph of him; nothing crosses projects unconfirmed | One small model call per relayed decision; more cards |
| Confirmed personal facts only | Leaner | No decisions in it |
| All decisions as they are | Richest | Project and client details cross projects (breaks 0277) |

## Recommendation
Confirmed personal facts plus generalized decisions.

## Choice
A personal graph (its own group) fed by personal memory lines he accepts, and by generalized decisions: after each relayed decision, Jarvis proposes a general version with no project or client names as a personal proposal card; only lines he accepts go in.

## Reason
"for global graph i think it would be useful like graph for jarvis/me"; then chosen.

## Consequences
- Amends 0280's personal-graph line.
- The leak check covers the personal group: it holds only accepted personal lines.
- The generalizing call uses the small model (0341).
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
