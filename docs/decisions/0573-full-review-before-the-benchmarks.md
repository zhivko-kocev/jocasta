# 0573 A full review and its fixes before the benchmarks; a week of use, then the benchmarks

- Status: v1
- Kind: process
- Phase/slice: program
- Vision: V13
- Date: 2026-11-09

## Question
CI passes on all three OSes (0572) and the benchmarks (0533) were next. Does anything come before them?

## Facts
- The harness-OS build (0532–0571) landed in one day; its reviews were the integrated live smoke and the phase checks, not a review of the whole repo.
- The product and the benchmark harness are about 29,000 lines of TypeScript and scripts, the tests about 21,000; `src/frontier/server/service.ts` alone is 4,267 lines.
- Every benchmark row measures the build it runs on (0533), so a change after the runs would leave them measuring an older build.

## Options
| Option | Pros | Cons |
|---|---|---|
| Full review and fixes first, a week of his use, then the benchmarks | The benchmarks measure the reviewed build he has used; findings from real use land before the runs | The benchmarks move to the weekend |
| Benchmarks first, review after | The runs start now | The runs measure a build the review may change; fixes after them aren't measured |

## Recommendation
Full review and fixes first.

## Choice
Before the benchmarks, a full review of the project from every aspect: security, UI and UX, bugs, refactors and improvements, speedups, duplicate code, clean code and design patterns, the tests, the shipped prompts and config, and the docs. Every finding is fixed: internal ones by the builders (0363), and the ones that change what he sees or keeps after his decision, each a record. Then he uses Jocasta through the week, and the benchmarks run at the weekend.

## Reason
His words: "before the benchmarks i want full full review on the project from every aspect, security, ui/ux, possible refactors/improvements/speedups (remove duplicate code apply clean code principles apply design patterns where possible ...etc) ..etc then when finished and everything fixed i will use jocasta this week and we run the benchmarks in the weekend".

## Consequences
- The benchmarks (0533) start at the weekend after his week of use, on the reviewed build.
- Refactors keep every decided behavior; a finding that would change one comes to him first.
- The review's findings and their outcome are kept in `docs/research/review-2026-11.md`.
