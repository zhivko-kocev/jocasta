# 0282 Phase 6 slice status

- Status: decided
- Kind: product
- Phase/slice: 6/opening
- Date: 2026-10-08

## Question
Which phase 6 slices are in v1 under lean Jarvis (0229)?

## Facts
Verified 2026-10-08: Pi 1.1.0's package ships example extensions `plan-mode/`, `subagent/`, `structured-output.ts`, `status-line.ts`, `git-checkpoint.ts` and `handoff.ts` (`examples/extensions`); Codemode with MCP and deferred tools is built in.
- His usage (appendix A): plan-mode switches 41; task 189, yield 1,399; hub wait 891, jobs 42; advise 221, TTSR 113 injections; eval 2,372, browser 164; web_search 756, `https://` reads 911; conflict 75; pr and issue 6; ast_edit 6; lsp 11.
- The recommendation kept 6.1, 6.3, 6.4, 6.5, 6.8 and 6.9; he asked to have the unticked slices explained, then added 6.2 and 6.6: "the rest we leave for after v1".

## Options
| Option | Pros | Cons |
|---|---|---|
| 6.1 plan mode | — | — |
| 6.2 model roles and fallbacks | — | — |
| 6.3 jobs, `wait`, `send` | — | — |
| 6.4 subagents | — | — |
| 6.5 advisor and TTSR | — | — |
| 6.6 LSP | — | — |
| 6.8 eval and browser | — | — |
| 6.9 code and web tools | — | — |
| 6.10, 6.12–6.15 | — | — |

## Recommendation
6.1, 6.3, 6.4, 6.5, 6.8, 6.9 v1; the rest after v1.

## Choice
- v1: 6.1 plan mode, 6.2 model roles and fallbacks, 6.3 jobs, `wait` and `send`, 6.4 subagents, 6.5 advisor and TTSR, 6.6 LSP, 6.8 eval and browser, 6.9 code and web tools.
- 6.7 MCP: Pi's built-in support as it is.
- After v1: 6.10 export and edit replay, 6.12 recon, 6.13 simulation, 6.14 forensics, 6.15 Iron Legion, and 6.11's remaining catalog rows.

## Reason
"the rest we leave for after v1"

## Consequences
- Differs from the recommendation: 6.2 and 6.6 are in v1.
- Phase 6's exit smoke drops recon.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
