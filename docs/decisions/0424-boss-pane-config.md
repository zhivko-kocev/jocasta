# 0424 The boss pane's own omp config

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
Slice G1 (0375) aimed at a simpler boss chat, which is gone (0409). Does the boss pane get its own omp settings?

## Facts
Verified 2026-10-14.
- The boss pane runs `omp --continue --append-system-prompt <config>/boss-manual.md` in `<data>/home` with no overlay; project panes already get one with `--config` (0400).
- omp has settings to hide thinking, collapse tool output, quiet startup and trim the status line (keys checked against 18.10.4 at build).

## Options
| Option | Pros | Cons |
|---|---|---|
| Ship config/boss.yml | A calm pane | One more shipped file |
| Plain omp | Nothing to keep | The boss looks like every thread |

## Recommendation
Ship config/boss.yml.

## Choice
`config/boss.yml` ships beside `boss-manual.md` and is passed to the boss pane with `--config`: thinking hidden, tool output collapsed, quiet startup, a short status line.

## Reason
"Ship config/boss.yml".

## Consequences
- Closes slice G1 of 0375.
