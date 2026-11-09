# 0535 The harness-OS build pins the omp that is latest when its build starts

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-08

## Question
The benchmarks ran on omp 18.13.1 (0500). Which omp does the harness-OS build (0532) and its benchmarks (0533) pin?

## Facts
Verified 2026-11-08 on his machine.
- His daily omp is 18.18.2 (`omp --version`). Its `--help` lists `--max-cost`, `--warn-cost`, `--max-time`, `--mode rpc|rpc-ui` with `--no-ui`, and the commands `collab`, `ps` and `find`.
- In 18.18.2's source: `lsp.shared` (default on) runs language servers through omp's daemon broker; `collab.autoStart` (`off|view|control`, default off) hosts every interactive session through `collab.relayUrl`, whose default is the public `my.omp.sh`; session files stay readable by Jocasta's 18.13.1-based reader.
- A bump re-checks plan 5.4's seams and 5.5's list (0383).

## Options
| Option | Pros | Cons |
|---|---|---|
| 18.18.2 | His daily omp; the newer native features | A seam re-check now |
| Stay on 18.13.1 | No re-check | The layer can't use the newer features; his daily omp differs from the build of record |
| Latest at build time | The newest features when the build starts | Unknown changes; seams re-checked when the build starts |

## Recommendation
18.18.2.

## Choice
The omp that is latest when the harness-OS build starts is pinned for the build and every benchmark on it; plan 5.4 and 5.5 are re-checked against it then.

## Reason
"Latest at build time".

## Consequences
- Differs from the recommendation.
- Amends 0500 (omp 18.13.1 for the runs).
- Facts in this program's records that cite 18.18.2 are re-checked against the pinned version before its build.
