# 0279 Who writes memory.md

- Status: decided
- Kind: product
- Phase/slice: 5/5.3
- Date: 2026-10-08

## Question
Who may write a project's `memory.md` and the personal store?

## Facts
- Claude Code lets the agent write auto memory freely (https://code.claude.com/docs/en/memory). His design calls `memory.md` curated, human-readable truth.

## Options
| Option | Pros | Cons |
|---|---|---|
| Proposals he confirms | Curated; no noise | A confirm step |
| The agent writes freely; he edits later | No friction | Noise |

## Recommendation
Proposals he confirms.

## Choice
Agents and the evaluator propose entries; he accepts, edits or rejects them, one at a time through the boss (0263). The same holds for the personal store (0277).

## Reason
Not given.

## Consequences
- None.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
