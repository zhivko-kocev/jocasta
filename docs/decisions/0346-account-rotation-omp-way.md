# 0346 Account rotation the omp way

- Status: decided
- Kind: product
- Phase/slice: 6/6.2 (amends 0311)
- Date: 2026-10-08

## Question
Pi keeps one login per provider. Where does Jarvis keep the extra accounts that 0311's rotation needs?

## Facts
Verified 2026-10-08.
- Pi 1.1.0 stores one credential per provider: `pi-ai/dist/auth/types.d.ts:31,43`; `pi-coding-agent/dist/core/auth-storage.d.ts:6` (`Record<string, Credential>`); a second `/login anthropic` replaces the first (`docs/providers.md:12,16`). `MainOptions` takes only `extensionFactories`, so no custom auth backend can be injected (0063).
- omp keeps several accounts per provider in its own storage, ranks them by usage (`packages/ai/src/auth-storage.ts:74`), and polls `GET https://api.anthropic.com/api/oauth/usage` for five-hour and seven-day utilization (`packages/ai/src/usage/claude.ts:68-71`).
- Jarvis already wraps Pi's Anthropic login and refresh (0343).

## Options
| Option | Pros | Cons |
|---|---|---|
| Extra accounts inside Pi's one credential | One file | Bends Pi's credential shape further |
| A Jarvis accounts file beside auth.json, swapped into Pi's entry | Clean separation | Two files in step |
| Drop rotation from v1 | Leanest | Amends 0311 |

## Recommendation
Extra accounts inside Pi's one credential.

## Choice
Now (0377): dropped; Pi's `/login anthropic` holds one account.

Originally: "however omp handles it": Jarvis keeps several accounts per provider in its own store, as omp does, ranks them by polled usage (five-hour and seven-day utilization from Anthropic's usage endpoint), and puts the chosen account into Pi's single credential before its limit hits; written fresh from omp's behavior.

## Reason
"however omp handles it"

## Consequences
- The login wrapper gains a way to add another account; the store's file and the switching threshold are built minimally and come back as as-built items.
- The strict profile's `rotateAccounts: false` applies.
- Differs from the recommendation.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
