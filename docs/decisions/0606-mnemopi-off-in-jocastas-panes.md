# 0606 Mnemopi is off in Jocasta's panes

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-14

## Question
Do the boss, the project panes and the thread panes keep omp's Mnemopi memory?

## Facts
Measured 2026-11-14 over the sessions since the cutover (`docs/research/state-of-the-art.md` section 4).
- At session start Mnemopi injected 7 blocks into this repo's sessions; 4 were non-empty, holding 3 distinct items. All three are earlier prompts of his, already carried out; none is a durable fact.
- The recall tool was never called in this repo's sessions. In other folders it was called 9 times: 4 answered "no relevant memories", 1 "not initialised", 4 returned transcript fragments that did not address the query in what was read.
- The boss's bank never got a row.
- Cost: about 330 fixed tokens a session and extraction calls on the `memory` role.
- In the literature, memory extracted automatically helped only when the right item was selected and caused failures otherwise (sections 4 and 10).
- After 0603 a project's lasting memory is its stored summaries, searched by `history_search` (0604), beside the files the project keeps.

## Options
| Option | Pros | Cons |
|---|---|---|
| Off for the boss and for project and thread panes | About 330 fixed tokens less a session, no extraction calls, nothing stale injected | No recall of raw past turns in panes |
| Off for the boss; in panes recall by tool only | Recall stays one call away | Its answers were fragments; extraction calls continue |
| Off for the boss only | Least change | Stale prompts keep being injected |

## Recommendation
Off for the boss and for project and thread panes.

## Choice
`memory.backend: "off"` in the boss's config and in the overlay every project and thread pane loads; the overlay's and the boss's `mnemopi` keys go. His own omp config is not changed by this record: omp started outside Jocasta keeps whatever memory he set.

## Reason
His answer: "off".

## Consequences
- Amends 0389's first half, 0400's bank per project and 0447; the `memory` role of 0591 is no longer used by Jocasta's panes.
- The `mnemopi.db` files already on disk stay where they are; nothing reads them from a Jocasta pane.
- V9's persistent memory is the SQLite store of 0603.
- Built in the overlay and `config/boss.yml`, with their test texts.
