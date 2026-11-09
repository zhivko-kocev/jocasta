# 0150 Spill large output

- Status: decided
- Kind: product
- Phase/slice: 1/1.8
- Date: 2026-10-07

## Question
What happens to large tool output?

## Facts
Verified 2026-10-07.
- Pi 1.0.1 keeps the last 2,000 lines or 50 KB inline and, for bash, names a temp file holding the full output (`dist/core/tools/bash.js:237-249`).
- The session file stores the hooked result (0084).
- Lesson L69 on Pi 1.0.1: a 1 MB result went from 51,336 characters inline to 170 with a 32 KiB spill.
- His omp history has 449 `artifact://` reads (appendix A). Appendix G recommends 32 KiB (v4: 50 KiB).

## Options
| Option | Pros | Cons |
|---|---|---|
| Spill at 32 KiB into the thread's dir, read back as `artifact://<n>` | Small windows; nothing lost; artifacts live with the thread | One more read for big outputs |
| Spill at 50 KiB | Fewer spills | Up to about 12k tokens inline per result |
| Pi's truncation only | Nothing to build | Non-bash output past the cap is lost; bash's full output sits in `%TEMP%` |

## Recommendation
Spill at 32 KiB.

## Choice
A tool result over 32 KiB is written to `<thread dir>/artifacts/<n>.txt` and replaced by a one-line notice; `read artifact://<n>`, with line ranges, reads it back.

## Reason
Not given.

## Consequences
- Notice, as previewed: `[output 1.0 MB, 18,204 lines spilled to artifact://7; read it with ranges]`.
- The internal URL router (appendix I) serves `artifact://`.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
