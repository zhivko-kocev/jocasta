# 0465 Rare boss procedures become skills

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
The boss manual is in every boss prompt. Where do the procedures the boss rarely needs live?

## Facts
Verified 2026-10-23.
- The boss manual (about 7 KB, `<data>/home/.omp/APPEND_SYSTEM.md`) is a context file in every boss request.
- omp skills keep only their name and description in the prompt and load the body on demand; for guidance that must always be present the docs say to use context files (https://omp.sh/docs/skills).
- Skills in the boss folder load from `<data>/home/.omp/skills/<name>/SKILL.md`.
- MCP server `instructions` are capped at 4000 characters (`src/sdk.ts:1408`), smaller than the manual.

## Options
| Option | Pros | Cons |
|---|---|---|
| Rare procedures as skills | A shorter always-on prompt; bodies load when needed | The boss must pick the skill |
| Keep everything in the manual | No change | Every request carries procedures it rarely uses |
| Move text into MCP instructions | One place with the tools | 4000-character cap; reaches subagents too |

## Recommendation
Rare procedures as skills.

## Choice
Procedures the boss rarely needs move from the boss manual into skills under `<data>/home/.omp/skills/` (only their names and descriptions stay in every prompt); the manual keeps what must always hold.

## Reason
"Rare procedures as skills".

## Consequences
- Amends 0331 (the manual's text).
- Which procedures move is chosen by the builder and listed as built; plan appendix I names the folder.
