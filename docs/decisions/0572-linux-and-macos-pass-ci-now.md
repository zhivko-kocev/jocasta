# 0572 CI's Linux and macOS jobs pass before the benchmarks

- Status: v1
- Kind: process
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
The first push ran CI (0051) on Windows, Ubuntu and macOS for the first time. Ubuntu failed 7 tests and macOS was cancelled. What should CI cover now?

## Facts
Seen on 2026-11-09 in run 42598380620, after the push of `ddf6f05`.
- Windows failed 4 tests, all in the tests, not the product: the install tests started Windows PowerShell from CI's PowerShell 7, which passes on PowerShell 7's module paths, so `Get-FileHash` failed to load (about_PSModulePath documents it); they compared an 8.3 temp path (`RUNNER~1`) with the long one PowerShell prints; and a canvas test read the reduced-motion durations of a machine with animations off.
- Ubuntu failed 7: bench's `toolCommand` test (path separators), two evals trials (a worker's `omp.jsonl` missing, a runner assertion), project identity of clones and worktrees (0104: the wrong main checkout picked), and three process-tree tests (0486, 0490).
- `docs/status.md` lists the Linux and macOS builds as untested and 0486's Linux and macOS process-tree paths as never run.
- No Linux machine with Bun is at hand: his WSL Ubuntu 24.04 has git but no Bun; macOS runs only on CI.

## Options
| Option | Pros | Cons |
|---|---|---|
| Windows only for now | main goes green with the test fixes; the benchmarks start sooner | Linux and macOS stay untested and CI says nothing about them |
| Fix Linux and macOS now | The three platforms the build ships for are checked on every push | Product work outside the plan's current step; the benchmarks wait |
| Keep all three, no fail-fast | Windows results show | main stays red until the ports land |

## Recommendation
Windows only for now.

## Choice
CI keeps Windows, Ubuntu and macOS. The failing Linux and macOS paths are fixed now, before the benchmarks, until all three jobs pass.

## Reason
"Fix Linux and macOS now".

## Consequences
- Differs from the recommendation.
- The benchmarks (0533) start after CI passes on all three.
- Closes, once green, the status open items on untested Linux and macOS builds as far as the suite reaches; `install.sh` and the release builds stay as untested as before.
- He approved Bun 1.4.3 in his WSL Ubuntu, user-local in `~/.bun`, for running the suite on Linux (2026-11-09; installed from the release zip with its SHA-256 checked, without `unzip`).
