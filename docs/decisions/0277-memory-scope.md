# 0277 Where memory lives

- Status: decided
- Kind: product
- Phase/slice: 5/5.2
- Date: 2026-10-08

## Question
Is memory per project, and is there a cross-project store?

## Facts
- The phase 5 goal makes the project thread the home of project memory. Scopes are one label per project (0262). His hand-written rules live in `~/.jarvis/agent/AGENTS.md` (0066).
- A cross-project store carries learnings everywhere but can leak client facts across projects if something other than him decides what is personal.
- His first answer asked "why no jarvis cross project store?"; a follow-up laid out the trade-off and a middle option.

## Options
| Option | Pros | Cons |
|---|---|---|
| Per project; preferences in his `AGENTS.md` | Sealed by construction | Learnings don't travel |
| Per project plus a personal store holding only memories he confirms | Learnings travel; no automatic leaks | A confirm step |
| Per project plus an automatic personal store | No confirm step | A model decides what is personal; leak risk |
| Per thread | — | Threads forget each other |

## Recommendation
Per project; preferences in AGENTS.md.

## Choice
Memory is per project, shared by its threads and never loaded in another project. A personal store, loaded in every project, holds only memories he confirms ("remember this everywhere", or a proposal he accepts), never project content.

## Reason
"why no jarvis cross project store?"

## Consequences
- Differs from the recommendation.
- The store's shape follows the memory design (5.0).
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
