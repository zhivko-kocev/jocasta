# 0339 omp drift check at every phase opening

- Status: decided
- Kind: process
- Phase/slice: 5/5.9 (section 4)
- Date: 2026-10-08

## Question
How does Jarvis's Anthropic shaping (0337) stay in step with omp?

## Facts
- `src/harness/anthropic-auth/omp-drift.ts` reads omp's files on GitHub at the pinned commit (89d2610) and at a newer ref, and lists the Claude Code values and subscription-related lines that changed, plus Claude Code's npm `latest`; it exits 1 when there is something to review.
- First run (2026-10-08): omp `main` seeds the billing suffix from the first conversational user turn on the wire; Claude Code's latest is 2.1.295, and Jarvis sends 2.1.280.
- Every phase opens with a fact check of versions and APIs (plan section 4).

## Options
| Option | Pros | Cons |
|---|---|---|
| Run it in each phase opening's fact check; port both changes now | A fixed schedule; no new command | Drift between phases waits for the next opening |
| A `bun run omp-drift` script run by hand; port both now | Any time | Easy to forget |
| Script only; port later | — | Stays on 2.1.280 and the old seed |

## Recommendation
At each phase opening; port both now.

## Choice
Now (0377): the drift check reads the npm metadata of `@gotgenes/pi-anthropic-auth`; a new version is bumped deliberately, and its peer range is checked on each Pi bump.

Originally: At each phase opening; port both now.

## Reason
"explain the recomended", then chosen.

## Consequences
- Plan section 4's phase-opening sources add the omp drift check; a found change is ported, written fresh, and the pin moves.
- The billing seed follows omp's first-conversational-user-turn rule, and the Claude Code version becomes 2.1.295.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
