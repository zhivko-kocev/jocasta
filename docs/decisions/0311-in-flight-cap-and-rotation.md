# 0311 In-flight cap and account rotation

- Status: decided
- Kind: product
- Phase/slice: 6/6.2
- Date: 2026-10-08

## Question
Does v1 cap parallel model requests across Jarvis processes, and rotate subscription accounts?

## Facts
- 0283 had put both levers after v1 without asking; this question closes that gap.
- Read 2026-10-08: Pi 1.1.0 retries transient failures at agent level (`retry.enabled` true, 3 attempts, backoff 2–60 s) and handles quota and usage-limit errors itself (`docs/settings.md:125-133`); its docs name no concurrency cap and no account rotation.
- omp: `providers.maxInFlightRequests` is a per-provider limit shared by every omp process with the same config root, unlimited by default; its docs' example sets `anthropic: 2` (`docs/settings.md:889,921`). Several OAuth accounts for one provider are ranked by polled usage and rotated automatically on usage limits (`docs/providers.md:57`, `docs/provider-quirks.md:178`).
- Frontier's threads and the boss share one process (0250); subagents are separate processes (0286). Appendix G proposed 6 for Anthropic.
- He asked how Pi and omp handle it, what the numbers mean, and about unlimited like omp.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's retry only; both after v1 | Nothing to build | Bursts stall in backoff; one account |
| omp-style shared cap in v1; rotation after | Fewer limit errors | A cross-process limiter |
| Cap and rotation in v1 | Parallel work survives limits | Most code; rotation needs 2+ subscriptions |
| Follow-up: cap 6, 3 or 10 by default | — | — |
| Follow-up: unlimited by default, a number optional | omp's behavior; fastest under the limit | Bursts rely on retries |

## Recommendation
Pi's retry only; both after v1.

## Choice
Now (0377): account rotation is dropped; Pi's `/login` holds one account per provider. The in-flight cap stands.

Earlier (0346): Pi keeps one login per provider, so the extra accounts live in Jarvis's own store, the omp way; the rest stands.

Originally:
- A per-provider cap on parallel model requests, shared by every Jarvis process, under `providers.maxInFlightRequests` in `settings.json`; empty means unlimited, the default. Requests over a set cap wait for a slot.
- Account rotation (`providers.rotateAccounts`, on by default): with several `/login` accounts for one provider in `auth.json` (0063), Jarvis polls each account's usage and moves to the next before the current one hits its limit. The strict profile (0186) turns it off.

```json
"providers": {
  "maxInFlightRequests": {},
  "rotateAccounts": true
}
```

## Reason
"how pi or omp handles this", "what does the numbers mean", "what about unlimited like omp?"

## Consequences
- Differs from the recommendation.
- Amends 0283's consequence that put both levers after v1.
