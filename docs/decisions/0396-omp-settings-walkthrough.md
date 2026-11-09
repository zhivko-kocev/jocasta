# 0396 The omp settings walkthrough

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-13

## Question
How are omp's settings chosen for Jarvis?

## Facts
Verified 2026-10-13.
- omp registers ~531 settings, shown in `/settings`; it reads `~/.omp/agent/config.yml`.

## Options
| Option | Pros | Cons |
|---|---|---|
| He walks every setting with a guide | His values, on evidence | Takes a session |
| Pick now from usage counts | Fast | No measurements or docs |

## Recommendation
Walk every setting with a guide.

## Choice
The agent writes a guide to omp's docs and settings; he walks every setting in his daily omp's `/settings`; his non-default values become a record, and `jarvis setup` writes them into a missing `~/.omp/agent/config.yml`.

## Reason
"i go throught everything and choose what i like from omp"; "we should read full omp docs for adapting omp for us".

## Consequences
- `docs/research/omp-adaptation.md` and `config/omp-config.yml`.
