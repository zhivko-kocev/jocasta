# 0588 Each benchmark script runs once before the full runs

- Status: v1
- Kind: benchmark
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
The review's fixes changed the live benchmarks' plumbing, and none of the scripts has run against a live Jocasta since. Run each once before the full runs, and where?

## Facts
Verified 2026-11-09 in the repo.
- H3, H5 and `bench/lib/launch.ts` now end process trees with the product's `killTree` (`src/shared/proctree.ts`), which returns once the tree is gone; H3's herdr and pane faults take t0 when it returns (`bench/h3-recovery.ts:364-365`, :396-397), about 50 ms later than the bench's own kill did, as its builder reported. The scripts find the service through the shared `readServiceFile` (`bench/h3-recovery.ts`, `h5-startup.ts`, `h6-memory.ts` and `bench/lib/jocasta.ts`, `sandbox.ts`, `threads.ts`).
- The tests run every script's dry run (`test/bench/scripts.test.ts:800-821`) and the scripts' parts, but no script against a live sandbox.
- H3 and H5 make no model call; H6 and H8 run threads' turns (`bench/README.md`, "Where they run").
- Every row of every benchmark runs again on this build (0533), so no earlier row is compared with these.

## Options
| Option | Pros | Cons |
|---|---|---|
| In the VM, at the weekend's start: each `--dry-run`, then H3 and H5 once and H6 and H8 once with their smallest settings | The real setting; anything broken is fixed before the long runs | About an hour of the weekend; a few dollars of thread turns |
| On his PC this week: H3 and H5 once in a throwaway sandbox | Breakage found this week | Windows open and close on his screen; H6 and H8 untested until the VM |
| Skip | No time or cost | A broken script shows mid-weekend and stops its run |

## Recommendation
In the VM, at the weekend's start.

## Choice
In the VM, before the full runs and in the sandbox: each script's `--dry-run`, then `h3-recovery.ts --runs 1`, `h5-startup.ts --runs 1`, `h6-memory.ts --threads 0,1` and `h8-parallel.ts --threads 2`, each into its own throwaway `--out`; anything broken is fixed before the long runs. These runs are checks, not results.

## Reason
He chose "VM, weekend start".

## Consequences
- `bench/README.md` lists the step before the full runs; 0482's runs and measures are unchanged.
