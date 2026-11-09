# 0219 Project trust mechanism

- Status: dropped
- Kind: product
- Phase/slice: 2/2.3
- Date: 2026-10-08

## Question
How does project trust work?

## Facts
Verified 2026-10-08 in Pi 1.1.0 (`docs/security.md:27-82`, `dist/core/trust-manager.js:173-174`, `dist/core/extensions/types.d.ts:522-537`):
- With `piConfig.configDir` `.jarvis` (`package.json`), Pi's trust gates `.jarvis/settings.json`, `.jarvis/mcp.json`, `.jarvis/extensions|skills|prompts|themes`, `.jarvis/SYSTEM.md` and `APPEND_SYSTEM.md`, and project `.agents/skills`.
- It saves one decision per canonical directory in `~/.jarvis/agent/trust.json`; the closest parent's decision applies.
- User-level and command-line extensions can decide through the `project_trust` event; an extension marked `builtin` loads after trust and can't.
- Gaps: a decision survives content changes, so a pulled commit that adds an extension loads without asking; worktrees at `<main-root>.wt/<tid>` (0118) are siblings of the main checkout, so its decision doesn't cover them.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's trust, with Jarvis's `project_trust` handler keying one decision per project id and hashing the protected files | Closes both gaps; Pi's prompt and `/trust` keep working | A hash store |
| Keyed by project, without hashes | Closes the worktree gap | Pulled extensions load silently |
| Jarvis's own trust table | Full control | Duplicates Pi |
| Pi's trust as it is | Nothing to build | Both gaps stay |

## Recommendation
Pi's trust with hashes, keyed by project.

## Choice
Now (0307): dropped; Pi's own per-folder trust decides.

Originally: Jarvis's `project_trust` handler decides: one decision per project id (0107) covering all its paths and worktrees, stored with a hash of each protected file in the catalog DB (0096). A new or changed protected file asks again.

## Reason
Not given.

## Consequences
- Jarvis's handler must load as a user-level or command-line extension, not `builtin`.
- Amended by 0307: dropped in the pruning pass; Pi's trust stays.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
