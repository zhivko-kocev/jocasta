# 0475 AGENTS.md keeps the rules; "Where we are" moves out

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-23

## Question
`AGENTS.md`, which every agent reads, carries a long "Where we are" history. Does it stay there?

## Facts
Verified 2026-10-23.
- `AGENTS.md`:58–79 is the "Where we are" status ledger, with lines thousands of characters long, ending in the rule "Update this section when a slice's records are written, when its build lands, and when a grilling session stops (name the next slice)."
- Omarchy's `AGENTS.md` is an index of task guides plus rules, with procedures in `agents/skills/` (https://github.com/basecamp/omarchy/blob/quattro/AGENTS.md, lines 1–25).

## Options
| Option | Pros | Cons |
|---|---|---|
| Rules stay; the history moves to `docs/status.md` | Every agent reads less; the history is still one link away | Churn in a file every agent reads |
| Keep it as it is | No change | The file keeps growing |

## Recommendation
Rules stay; the history moves to `docs/status.md`.

## Choice
`AGENTS.md` keeps the rules, the package map and conventions, plus a short current-state line; the long "Where we are" history moves to `docs/status.md`, which AGENTS.md points to; the "Update this section…" rule moves with it.

## Reason
"Split the repo's AGENTS.md: rules stay, the long 'Where we are' history moves out".

## Consequences
- Amends AGENTS.md's "Where we are" convention: the history is kept in `docs/status.md`.
- AGENTS.md's precedence rules stay as they are.
