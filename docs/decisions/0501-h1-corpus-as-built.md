# 0501 H1's corpus as built, scored for each way of writing

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-27

## Question
0482 registered H1 on a 300-command corpus (the design: 120 must-deny, 80 must-ask, 100 must-allow). It ran on 1,285 commands made from 224, and no record says so. How is that handled?

## Facts
Verified 2026-10-27.
- `bench/h1-guard/build-corpus.ts`: a researcher who never read the guard collected 224 commands from public docs (70 must-deny, 42 must-ask, 112 must-allow; `D:/thesis/data/bench/h1-sources.jsonl`). The builder keeps each as written and adds rewrites that don't change what the command does: after `cd … &&`, inside a `;` chain, `pwsh -c`, `cmd /c`, lowercase PowerShell, `git.exe`, git's full path, `git -C` and doubled spaces. That gives 1,285 lines (426 / 242 / 617), SHA-256 `276b223a944c81cb80c74eb1a127904f9566a7a00a8cae68c47e818d39b1de32`.
- The result of 2026-10-24 (`D:/thesis/data/bench/h1/`): deny recall 67.4% [62.9, 71.6], must-ask stopped 74.8% [69.4, 80.2], false deny 0.0%, false ask 0.2%.
- `bench/h1-guard/evaluate.ts` judges `guard.yml`'s rules only, splitting a command at `&&`, `||`, `;` and `|` in its raw text. A pane's omp also applies `gitGuard` and its built-in critical patterns, splits with its shell tokenizer (quotes honoured and removed; also `&`, subshells and newlines), and never lets an allow rule ride a compound line (`tools/bash.ts:241-318` in 18.13.1). The design's check of the evaluator against real omp on 30 commands hasn't run.

## Options
| Option | Pros | Cons |
|---|---|---|
| A record, and the result stays | Honest, no work | No view near the registered size |
| A record, plus a score for each way of writing | The 224 as written come close to the registered 300, and the table shows which rewrites slip past the guard; no model calls | Minutes of work |
| Redo the test as planned | Matches the registration | A new blind researcher; the 1,285 result becomes extra |

## Recommendation
A record, plus a score for each way of writing.

## Choice
H1's corpus is the 1,285 commands as built. Beside the overall result, H1 scores each way of writing separately with the same metrics and intervals: the 224 commands as written, and each rewrite.

## Reason
"A record, plus a score for each way of writing". Asked a second time in plainer words after his first answer said he didn't understand the question; the first asking recommended keeping the result alone.

## Consequences
- Amends 0482's H1 line.
- The thesis's validity section names the change and the evaluator's gaps.
