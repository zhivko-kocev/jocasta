# 0165 Eval runs and results

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
How are eval trials isolated, and how are results kept?

## Facts
Verified 2026-10-07 (0079): leftover state from an earlier trial can hand over the answer; the claude-api skill writes per case a `results.jsonl` row (model read from the response, usage, grade) and one transcript per rep, and has a report page.

## Options
| Option | Pros | Cons |
|---|---|---|
| A fresh fixture per trial and the claude-api skill's layout | Reuses the skill's report and climb; no leakage | Fixture setup per trial |
| Jarvis's own format | Shaped to Jarvis | Rebuilds the tooling |

## Recommendation
A fresh fixture and the skill's layout.

## Choice
Each trial runs in a fresh temp fixture repo; results follow the claude-api skill's layout: `results.jsonl` (case, rep, model, usage, grade, passed), `transcripts/<case>-<rep>.jsonl` and `report.html` per run.

## Reason
Not given.

## Consequences
- The previewed run folder was `<config>/evals/runs/<time>/`; 0095 puts machine-written data in the data dir, so the location is asked again with the next 1.10 questions.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
