# 0063 Credential storage

- Status: decided
- Kind: product
- Phase/slice: 0/0.4
- Date: 2026-10-04

## Question
Where are credentials stored in v1?

## Facts
- Pi 1.0.1 `main()` builds credentials through `ModelRuntime.create({ authPath })`; `MainOptions` has only `extensionFactories`.
- `AuthStorageBackend` is one blob behind `withLock`/`withLockAsync`; the default writes `<config dir>/auth.json` under a `proper-lockfile` lock.
- Tested 2026-10-04: `Bun.secrets` round-trips up to exactly 2560 bytes in Windows Credential Manager; 2561 fails with code 1783. Test entries were deleted.

## Options
| Option | Pros | Cons |
|---|---|---|
| OS keyring | Tokens not in a plain file | No way into `main()` yet; chunking above 2560 bytes |
| Pi's `auth.json` | Works today, no code | Plaintext file in the home dir |

## Recommendation
Keyring, with `auth.json` until a route into `main()` lands.

## Choice
Pi's `auth.json` in `~/.jarvis/agent`.

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- Slice 0.4's decisions 2–5 (how a backend gets in, keyring library, values above 2560 bytes, locking) no longer apply and were not asked.
- Keyring credentials are no longer a fork-condition candidate (plan 5.5).
- The phase 0 exit smoke's Credential Manager check doesn't apply.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
