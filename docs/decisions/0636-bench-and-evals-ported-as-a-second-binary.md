# 0636 bench/ and evals/ are ported to Rust as a second binary

- Status: v1
- Kind: product, benchmark
- Phase/slice: program
- Date: 2026-11-15

## Question
0625 left `bench/`, `evals/` and `scripts/` to the start of the port, with his rule: they stay TypeScript where they must, and are ported otherwise. What is ported?

## Facts
Verified 2026-11-15.
- **H1's oracle has to stay TypeScript.** It loads omp's own guard code (`bench/h1-guard/omp-verdicts.ts`).
- **Size and imports.** `bench/` and `evals/` are 12,592 lines, and they import about 6,800 lines of product code: the herdr client, sessions, ledger, checks, proctree, service-file, fsx and process, among others. After the cutover, that code no longer exists in TypeScript.
- **A6's layer** (`src/frontier/layer/bench.ts`) is product code that the bench loads (0543).
- **0588** already runs each benchmark script once in the VM before the full runs.

## Options
| Option | For | Against |
|---|---|---|
| Ported to Rust as a second binary in the same package, built from the product's modules; H1's oracle stays a TypeScript file run with Bun | one herdr client, ledger and session reader; no drift between product and harness | 12,600 more lines before the benchmarks |
| Stay TypeScript, with copies of the 6,800 lines in `bench/lib` | the harness checked in the VM stays as it is | two implementations of the same readers; a format change breaks the harness quietly |
| Stay TypeScript and reach the product only through its CLI, HTTP and documented files | independent of the product's code | new surfaces for what the harness reads today, and a rework |

## Recommendation
Ported as a second binary.

## Choice
- **`bench/` and `evals/` become a second binary of the `jocasta` package,** built from the product's own modules. That includes A6's layer.
- **H1's oracle** stays a TypeScript file, run with Bun.
- **`scripts/build.ts`** becomes a mise task: cargo, plus Bun bundling the canvas page.
- **`scripts/scorecard.ts` and `scripts/omp2-watch.ts`** stay TypeScript; they are small dev scripts run by mise.
- **`scripts/install.ps1` and `scripts/install.sh`** stay shell scripts.

## Reason
His answer: `rust-second-binary`.

## Consequences
- This record amends 0625 (what of `bench/`, `evals/` and `scripts/` is ported).
- **Entry points.** `mise run evals` and the bench's documented commands name the new binary. `bench/README.md` changes with them.
- **`typebox`** leaves `package.json` with the evals.
- **The ported harness** runs each script once in the VM before the full runs (0588).
