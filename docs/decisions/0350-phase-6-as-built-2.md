# 0350 Phase 6 as-built, rotation and search

- Status: decided
- Kind: product
- Phase/slice: 6/6.2, 6.9
- Date: 2026-10-08

## Question
Are the rotation and search details as built accepted?

## Facts
- Built per 0346 and 0347, tested offline against fake endpoints.
- Pi 1.1.0 has no public way to replace the credential it reads per request; `ModelRuntime.credentials` is private.

## Options
| Option | Pros | Cons |
|---|---|---|
| Accept as built | Done | — |
| Rework | — | — |

## Recommendation
Accept as built.

## Choice
- Extra accounts live in `~/.jarvis/agent/accounts.json` (private file). Usage is polled per account at most every 5 minutes, plus the usage headers on every reply. Jarvis switches when the active account passes 85% of its five-hour or seven-day window, or hits a rate limit (Pi's retry then goes out on the next account), and never switches without need. Now (0377): dropped with rotation.
- Adding an account is `/login anthropic` with the other account; Jarvis asks "Add this account (keep N more for rotation)" or "Replace the other accounts with this one". Now (0377): dropped.
- `web_search` order: Anthropic web search (up to 5 searches per call, 90 seconds), then DuckDuckGo and Mojeek (20 seconds each), then Brave with `BRAVE_API_KEY`; each fallback adds a `Note:` line; the result is the answer, numbered sources, citations and a `Provider:` line.
- Switching accounts writes Pi's credential through the private `ModelRuntime.credentials`, checked at runtime and re-checked at every Pi bump. Now (0377): dropped.

## Reason
All ticked.

## Consequences
- Plan 5.5's non-documented seams add `ModelRuntime.credentials`.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
