# 0185 Profiles mechanism

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
How do profiles work?

## Facts
Verified 2026-10-07: Pi 1.0.1 deep-merges the global `<config dir>/settings.json` with the project's `.jarvis/settings.json`; it has no profile mechanism and no `--profile` flag.
- 0137: Jarvis never takes Pi's names. Principle 3: one owner per setting, deltas only.

## Options
| Option | Pros | Cons |
|---|---|---|
| Profile files in the config dir, layered over `settings.json`, chosen with `--profile <name>` | One place per profile; reproducible runs | A third settings layer |
| Flags only | No files | Long command lines; boss and worker settings scattered |

## Recommendation
Profile files and `--profile`.

## Choice
`~/.jarvis/agent/profiles/<name>.yaml` holds only the deltas over `settings.json`; `--profile <name>` picks one, and Frontier passes the boss or worker profile to the tabs it starts.

## Reason
Not given.

## Consequences
- Appendix I claims `--profile`; the profile mechanism seam (1.12) is this.
- Preview, illustrative: `version: 1`, `thinking: high`, `model_fallback: off`, `budgets: { turns: 40, spend_usd: 5, wall_clock: 30m }`; the strict profile's contents and the budget keys are decided with 1.12's remaining items.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
