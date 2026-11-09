# 0220 What project trust gates

- Status: decided
- Kind: product
- Phase/slice: 2/2.3
- Date: 2026-10-08

## Question
Beyond Pi's own list, which project resources need trust before Jarvis loads them?

## Facts
- Pi loads `AGENTS.md` and `CLAUDE.md` regardless of trust (`docs/security.md:57`); Pi's own list is in 0219.
- Claude-format `.mcp.json` servers are commands Jarvis would start. Project `permissions.yaml` can't weaken user denies (0197), but its allow rules can turn asks into allows. Claude Code holds repository-supplied allow rules until the folder is trusted (https://code.claude.com/docs/en/permissions, 2026-10-08).
- His first multi-select ticked only `.claude` hooks; a follow-up named the two risks above and he ticked `.mcp.json` servers.

## Options
| Option | Pros | Cons |
|---|---|---|
| `.claude` hooks (2.5) | — | — |
| `.claude` commands and skills (0154) | — | — |
| `.jarvis/permissions.yaml` (0196) | — | — |
| Project MCP servers in `.mcp.json` (6.7) | — | — |
| `AGENTS.md` and `CLAUDE.md` | — | — |
| Follow-up: gate `.mcp.json` servers | Closes code execution on open | — |
| Follow-up: gate `permissions.yaml` allows only | A cloned repo can't loosen asks | — |

## Recommendation
None was marked in the questions.

## Choice
Now (0307): Pi's own trust gates Pi's list; no file hashes (0219 dropped).

Earlier (0231): Pi's list only; `.claude` hooks and `.mcp.json` aren't loaded at all.

Originally:
Gated, besides Pi's list: `.claude` hooks and `.mcp.json` servers. Load without trust: `.claude` commands and skills, `.jarvis/permissions.yaml` (all its rules), `AGENTS.md` and `CLAUDE.md`.

## Reason
Not given.

## Consequences
- Amends 0196: the project `permissions.yaml` loads in every project, not only trusted ones; a cloned repo's allow rules apply without asking.
- Their hashes join the trust decision (0219).
- Amended by 0231: only Pi's list is gated; `.claude` hooks and `.mcp.json` aren't loaded at all.
- Amended by 0307: Pi's trust gates the resources, without hashes.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
