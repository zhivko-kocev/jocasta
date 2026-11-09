# 0545 Fresh context after the plan and Ponytail's conditional test block are measured in the follow-up study

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-08

## Question
HumanLayer's advanced context engineering keeps context use low by starting implementation fresh from an approved plan, and its CLAUDE.md guide reports better adherence from `<important if="…">` blocks. Does Jocasta build either, or measure them?

## Facts
Verified 2026-11-08.
- omp 18.18.2's plan review offers "Approve and execute", "Approve and compact context", "Approve and keep context (~tokens / window)" and "Refine plan" (`modes/interactive-mode.ts:7820-7824`); only "execute" drops the research context (`preserveContext: choice !== "Approve and execute"`, `:8015`), and it is the picker's default (Jocasta relies on Enter picking it, `src/frontier/server/service.ts:1922`).
- Under 0540 the brief and `done_when` sit in the thread's system prompt, so they survive a fresh start.
- HumanLayer: "Frequent intentional compaction… keeping utilization in the 40–60% range" (advanced context engineering); "`<important if=\"condition\">` gave noticeably better adherence", without a rigorous explanation (writing a good CLAUDE.md).
- Ponytail's "Running tests" rules are 5 lines (0469).

## Options
| Item | Options (pros / cons) |
|---|---|
| Fresh context | Nothing to build, measure "execute" against "keep context" · Nothing to build, no measure |
| Ponytail block | Measure in the follow-up study · Adopt now (anecdotal) · Skip |

## Recommendation
Nothing to build, measured; Ponytail's block measured.

## Choice
- Nothing is built for a fresh context: omp's default approval already starts implementation without the research context. The follow-up study (0543) compares "Approve and execute" with "Approve and keep context" on A6, paired, by cost per passing task.
- The follow-up study also compares Ponytail with its "Running tests" rules inside `<important if="you are running or writing tests">` against plain Ponytail; the block is kept by 0543's rule.

## Reason
"Nothing to build; measure"; "Measure in the follow-up".

## Consequences
- Amends 0543: the follow-up study gains these two comparisons.
- Amended by 0567: A6 answers the plan picker with "Approve and execute" and the variant with "Approve and keep context", since `--plan-yolo` keeps context.
