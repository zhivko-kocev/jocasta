# 0386 The harness is omp plus config

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Which codebase is Jarvis's harness, and how much Jarvis code runs inside it?

## Facts
Verified 2026-10-13.
- omp (`can1357/oh-my-pi`) 18.10.3, npm `@oh-my-pi/pi-coding-agent` (2026-10-12), MIT; TypeScript ~980K and Rust ~355K lines, natives prebuilt per platform, `omp-windows-x64.exe`. It descends from Pi.
- Jarvis pins Pi (`earendil-works/pi`) 1.1.0, MIT.
- ~6.4K of Jarvis's 8.9K harness lines re-implement omp features; he ran 658 omp sessions in August–September (appendix A).
- omp's system prompt is 13.1K characters and its 76 tool prompts 83.9K characters.
- omp has approvals (`tools.approvalMode`, `tools.approval.<tool>`), memory backends, subagents, plan mode, LSP, eval, browser, web search, ask, todo, advisor, TTSR, model roles, compaction, `/handoff`, redaction and logins.

## Options
| Option | Pros | Cons |
|---|---|---|
| omp + config, Jarvis code only outside omp | No Jarvis code to rebase or keep in step with omp; his daily omp is the harness | Loses the named git-guard deny rules and the enforced chat-only trunk |
| Same, plus the guard as one omp extension | Keeps the deny rules and chat-only trunk | The only Jarvis code inside omp, rebased on every omp bump |
| Keep the current plan | Ports Jarvis's extensions onto omp | Re-builds what omp has |
| omp fork, stripped | Smaller binary | A rebase stream with strip patches |

## Recommendation
omp + config, Jarvis code only outside omp.

## Choice
The harness is whole omp, unforked, adapted with settings, config files and memory only; no Jarvis code runs inside omp. `src/harness` is deleted; Pi, `billion-context` and `@gotgenes/pi-anthropic-auth` leave. Retired Jarvis harness features are covered by omp's own: subagents, plan mode, LSP, eval, browser, web search, ask, todo, advisor, TTSR, roles, compaction, redaction, logins.

## Reason
"the harmess is omp wh ydo we add jarvis there, shouldnt we just tweak settings + memory?"; "omp + config, Jarvis code only outside omp"; "i think i want the whole omp put customized for me i go through each setting and tweak".

## Consequences
- Plan 5.1–5.5 describe omp as the harness; phases 1–3 and 6 lead with this record.
- Principle 8 (minimal by default) is bent: whole omp is kept by his word, its settings tuned in the walkthrough (0396).
- The eval runner runs his omp (F1).
