# 0420 0398's omp values are set by him

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-14

## Question
His `~/.omp/agent/config.yml` exists without 0398–0400's values, and `jarvis setup` writes that file only when it is missing. Does setup merge them in?

## Facts
Verified 2026-10-14.
- His config.yml has none of 0398's values: no `bash.patterns` (the `git push --force` deny), memory backend `local` not `mnemopi`, telemetry and update check at defaults.
- So threads and the boss run without 0398's guard patterns until they are set.

## Options
| Option | Pros | Cons |
|---|---|---|
| Setup offers a merge | The guard is active after one `y` | Setup edits a file he already owns |
| He sets them in /settings | His file stays his | The guard patterns are off until he does |

## Recommendation
Setup offers a merge.

## Choice
Setup stays as built (writes `config.yml` only when missing). He sets 0398–0400's values himself in omp's `/settings`, from `config/omp-config.yml`.

## Reason
"I'll do it in /settings".

## Consequences
- Differs from the recommendation.
- Until he does, 0398's guard patterns are not active.
- Amended by 0451: the reference config he sets gains more values and documents keybindings, `WATCHDOG.md` and `lsp.json`.
- Amended by 0466: `jarvis setup` merges the shipped omp settings into his existing `config.yml` key by key, shown as a diff and applied after his `y`.
- Amended by 0593: after his `/settings` pass the agent wrote the values into his `config.yml` on 2026-11-10.
