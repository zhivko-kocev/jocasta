# 0280 The boss's memory

- Status: decided
- Kind: product
- Phase/slice: 5/5.2, 4.4
- Date: 2026-10-08

## Question
How does the boss learn to think like him?

## Facts
- His words: "maybe we can do the same for the main orchestrator so we can have opy of ourselves in the jarvis itself. it will start maybe to think like me".
- The boss sees states, titles, report summaries and questions, never thread content (0262), relays questions and never answers them (0259). The personal store holds only confirmed entries (0277).
- His answers to relayed questions can name clients; scope labels seal by project (0262).

## Options
| Option | Pros | Cons |
|---|---|---|
| The personal store plus a decision log of relayed questions and his answers, sealed to each project's scope; relays show a predicted answer | Learns his choices without leaking or answering for him | — |
| The same, and it answers itself when confident | Fewer interruptions | Reopens 0259 |
| The personal store only | Smallest | No learning from his answers |

## Recommendation
As described.

## Choice
Now (0342): the personal graph takes only personal lines he accepts and generalized decisions he accepts, never the raw log.

Originally: the boss uses the personal store (0277) and keeps a decision log: each question it relays and his answer, sealed to that project's scope label and used only for that project. When it relays a question, it shows the answer he would probably give, citing the earlier decision; it still never answers for him (0259). If the Graphiti spike keeps the graph (0278), the log also feeds a personal graph of his decisions.

## Reason
"maybe we can do the same for the main orchestrator so we can have opy of ourselves in the jarvis itself. it will start maybe to think like me"

## Consequences
- Preview: `you'd probably say v2 (like 2026-09-30 in api: "always newest API")`.
- The log lives in each project's `project.db`.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
