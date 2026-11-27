# 0693 omp is not pinned

- Status: v1 (built 2026-11-27)
- Kind: product, process
- Phase/slice: ultron 0.3 iteration, J10
- Date: 2026-11-26
- Amends: 0690 (the product's omp pin becomes "last checked with")

## Question
0690 pins omp 19.2.0: the README and manual name it, ultron installs that version, and the guard test's oracle refuses any other. He wants the latest omp always. How does Jocasta follow omp without a pin, and how does it learn that a new omp broke something?

## Facts
- Verified 2026-11-27: his omp is 19.2.1 (mise's `oh-my-pi`, the GitHub release binary), the newest on npm too. `omp config list --json` prints every registered setting as `{"<dotted key>": {type, value?, description}}` (648 on 19.2.1); map settings such as `modelRoles`, `retry.fallbackChains` and `providers.maxInFlightRequests` have the type `record`.
- The settings Jocasta writes come from `config/omp/config.yml`, `boss.yml`, `guard.yml`, `headless.yml`, `heavy.yml`, `review.yml` and `overlay_text` (`src/frontier/server/overlay.rs`); 99 keys in all, every one registered in 19.2.1.
- `bench/h1-guard/omp-verdicts.ts` loads omp's own modules from its npm package and refuses a version other than `PINNED_OMP` unless asked with `--any-version` (`anyVersion` in its `load` request). On 19.2.1's npm package (installed into a scratch `BUN_INSTALL`), `test/config/guard.test.ts` passes all 15 tests with the oracle; without a package its 7 oracle tests skip.
- A mise release binary carries no TypeScript sources, so the oracle needs the npm package of the same version beside it.

## Options
| Option | Pros | Cons |
|---|---|---|
| "Last checked with omp X" wording, an `omp-seams` task and a weekly workflow on the newest omp that opens an issue on a break | Users run the latest omp; a break is reported within a week, without a human checking | A break reaches users before it is fixed |
| Keep the pin and bump it by hand | Every release names a tested omp | ultron and users lag behind omp; each bump is a manual re-check |

## Recommendation
No pin; check weekly.

## Choice
His answers of 2026-11-26 and the approved plan:
- README and manual say Jocasta follows omp's latest release and name the omp a release was last checked with (19.2.1 for 0.1.0).
- `test/config/guard.test.ts` keeps `PINNED_OMP` as that last-checked version and asks the oracle for any version.
- `mise run omp-seams` (`scripts/omp-seams.ts`) runs `omp config list --json` with a scratch agent folder and fails naming every key of the files above that omp doesn't register; a key under a map setting is checked at the map's level. `overlay_text`'s keys are listed in `scripts/omp-seams-overlay-keys.txt`, which a Rust test keeps equal to the overlay.
- `.github/workflows/omp-latest.yml`, weekly (Mondays 06:00 UTC) and on demand: installs the newest omp with mise and its npm package of the same version, runs `mise run omp-seams` and the guard test (a skipped oracle test counts as a failure), and on failure opens the issue `omp <version> breaks a seam` with the logs' tails, unless an open issue has that title.

## Reason
He uses the newest omp; a weekly machine check replaces the hand bump.

## Consequences
- 0690's pin no longer holds for the product. H1's benchmark keeps its own `PINNED_OMP` in `omp-verdicts.ts` and `omp_verdicts.rs` (0535): a paid run must not mix omp versions.
- The source citations in comments keep naming the omp version they were read in.
- `omp-seams` is not part of `mise run check`: it needs omp on PATH, which CI's check jobs don't have.
