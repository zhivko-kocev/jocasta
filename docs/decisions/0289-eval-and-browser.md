# 0289 Eval and browser tools

- Status: decided
- Kind: product
- Phase/slice: 6/6.8
- Date: 2026-10-08

## Question
Which code-running and browser tools does Jarvis have?

## Facts
Verified 2026-10-08 in Pi 1.1.0 (`docs/codemode.md:1-33`): Codemode runs a JavaScript script in a QuickJS sandbox with no file system, network or Node APIs; it calls other tools, and only its output reaches the model.
- Python 3.14.2 and Edge are installed (checked 2026-10-08). His usage: eval 2,372 (Python 1,925, JS 447), browser 164 (appendix A).
- He asked why Python is needed and what Codemode is; both were explained, including that 1.10's benchmark evals don't depend on the Python tool.

## Options
| Option | Pros | Cons |
|---|---|---|
| Codemode, a persistent Python process per thread, and CDP driving Edge | No new installs; his habits | Three tools |
| Codemode and the browser only | Smaller | No stateful Python |
| A persistent JS runner too | — | Duplicates Codemode |

## Recommendation
Codemode, persistent Python and CDP Edge.

## Choice
JavaScript goes through Pi's Codemode as it is; a `python` tool keeps one Python process per thread so state survives between calls; a browser tool drives Edge headless over CDP in its own profile (open, click, type, screenshot, evaluate).

## Reason
Not given.

## Consequences
- None.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
