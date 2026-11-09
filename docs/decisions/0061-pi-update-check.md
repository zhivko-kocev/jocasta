# 0061 Pi's update check

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Date: 2026-10-04

## Question
Does Jarvis keep Pi's update check on start? (Left to 0.3 by 0048.)

## Facts
- Pi 1.0.1 `utils/version-check.js`: every TUI start fetches `https://pi.dev/api/latest-version` with a User-Agent carrying the version.
- The 0.3 smoke (2026-10-04) showed the check comparing Pi's latest (0.73.1) with Jarvis's version (0.1.0), so it printed a false "New version 0.73.1 is available. Run jarvis update".
- On Windows, `jarvis update` refuses to update itself ("self-update on Windows is only supported for npm and pnpm installs") and still updates Pi packages.
- `PI_SKIP_VERSION_CHECK` turns only this check off.

## Options
| Option | Pros | Cons |
|---|---|---|
| `src/entry.ts` sets `PI_SKIP_VERSION_CHECK=1` before `main()` | No false banner; no request to pi.dev on start | No notice of new Pi releases; the pin moves on purpose anyway (0041) |
| Keep Pi's check | Nothing to build | A wrong banner on every start |

## Recommendation
Turn it off in the entry.

## Choice
`src/entry.ts` sets `PI_SKIP_VERSION_CHECK=1` before `main()`.

## Reason
Not given.

## Consequences
- `jarvis update` still updates Pi packages (0045).
- Upgrading Jarvis itself is 0050 (after v1).
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
