# 0532 The harness-OS layer is v1, built before the benchmarks

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V1, V3, V8, V13
- Date: 2026-11-08

## Question
He asked Jocasta to tackle where coding agents waste tokens from outside the harnesses, "like one big harness or maybe even an OS for multiple harness processes", and to prefer open-source tools over building. `docs/research/harness-os.md` proposes K1–K11 plus omp and opencode drivers. What is its status, and when is it built?

## Facts
Verified 2026-11-08.
- Paired studies: compressing tool output made the bill go up (+6.8% in arXiv 2607.12161; rtk +7.6% at low effort in JetBrains' benchmark); filtering tool definitions is the one lever that pays reliably (arXiv 2609.22114); a warm shared code-intelligence daemon made agents 20–30% more efficient (Netflix and JetBrains, 2026-10). Links in `docs/research/harness-os.md`.
- The full task run is paused since 2026-11-05 13:20Z with 4 of 450 rows (`docs/status.md`); H1–H9 are done on the build `c154055b…004b` (0520, 0530).
- In the brainstorm he answered "we make the changes then we continue from start", picked all of K1–K11, and drivers for omp and opencode with Claude Code read only.

## Options
| Option | Pros | Cons |
|---|---|---|
| v1, before the benchmarks | The thesis measures the real product; only 4 task rows are lost | Weeks of building before any result; risk to the thesis date |
| v1 in two steps (K1, K4, K7 first) | Results sooner | The thesis measures half the layer; two rebuilds |
| After v1 | Fastest to results | The thesis can't claim the layer |

## Recommendation
v1, before the benchmarks.

## Choice
v1, before the benchmarks. K1–K11 and the omp and opencode drivers are grilled one decision at a time in this program and built before any benchmark runs again.

## Reason
"v1, before the benchmarks".

## Consequences
- Each K item and the drivers get their own records; `docs/research/harness-os.md` is their source.
- The paused full run doesn't resume; 0533 says what runs again.
- Plan appendix I gains the layer's seams when their records are written (0027).
