# 0317 Eval case format

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-08

## Question
How are eval cases written and run?

## Facts
- Built for 1.10 without a record: a folder per case with `case.yaml` (prompt, tags, end-state checks: `command`, `file` with `contains`/`absent`, `git_clean`), `script.yaml` for the fake model, and `fixture/` seeded into a fresh temp git repo per trial; results in `<data>/evals/runs/<time>/` (`results.jsonl`, transcripts, `report.html`); his real cases in `~/.jarvis/agent/evals/cases/`.
- He asked why evals are needed; it was explained (0229, 0230).

## Options
| Option | Pros | Cons |
|---|---|---|
| As built, plus a `bun run evals` script | One command | — |
| As built, no script | — | Longer command |

## Recommendation
As built, plus the script.

## Choice
Now (0376): as built; scripted cases install the scripted-model test fixture into each trial's config.

Originally: As built, run with `bun run evals` (`bun evals/run.ts`).

## Reason
"why do we need this"

## Consequences
- CI runs the scripted cases through `bun test` (0169).
