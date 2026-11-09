# 0058 Telemetry off: mechanism

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Date: 2026-10-04

## Question
How is install telemetry kept off (0048)?

## Facts
Pi 1.0.1 `core/telemetry.js`: `PI_TELEMETRY` in the environment overrides the `enableInstallTelemetry` setting. `piConfig` doesn't rename Pi's env vars.

## Options
| Option | Pros | Cons |
|---|---|---|
| `src/entry.ts` sets `PI_TELEMETRY=0` before `main()` | Pinned; no setting or UI toggle turns it on | One line kept across Pi bumps |
| `enableInstallTelemetry: false` in the seeded settings | Visible in the file | Can be flipped back on |

## Recommendation
The env var in the entry.

## Choice
`enableInstallTelemetry: false` in the settings seeded on first run (0056).

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- Telemetry stays on if `settings.json` already exists before Jarvis first runs (no seed), or if the setting is changed later; 0048 is then not met for that machine.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
- Amended by 0396: omp's settings come from his walkthrough and `jarvis setup` writes them.
