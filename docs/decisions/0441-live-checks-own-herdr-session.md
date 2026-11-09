# 0441 Live checks in their own herdr session

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-15

## Question
The polish round's live walkthrough must start and close Jarvis repeatedly. How does it avoid his herdr session `jarvis`?

## Facts
Verified 2026-10-15.
- `jarvis` and the companion use herdr session `jarvis` (`HERDR_SESSION` in `src/shared/cli.ts`); his session lives in `%APPDATA%\herdr\sessions\jarvis`.
- Closing stops herdr session `jarvis` (0434), so a live check on it would stop his work.

## Options
| Option | Pros | Cons |
|---|---|---|
| Test-only JARVIS_HERDR_SESSION | Live checks never touch his session | A new env var |
| Separate Windows user | No new seam | Heavy; his omp login missing |
| Only unit tests | Nothing new | Start and close never seen end to end |

## Recommendation
Test-only JARVIS_HERDR_SESSION.

## Choice
Test-only env var `JARVIS_HERDR_SESSION` (default `jarvis`), read by `jarvis` and the companion, so live checks run in a throwaway session and never touch his session `jarvis` (`%APPDATA%\herdr\sessions\jarvis`).

## Reason
"Test-only JARVIS_HERDR_SESSION".

## Consequences
- Adds a test seam; plan appendix I lists the env var.
- A value that isn't letters, digits, `.`, `_` or `-` (64 at most) stops `jarvis` with exit code 2.
