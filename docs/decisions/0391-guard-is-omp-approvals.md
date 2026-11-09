# 0391 The guard is omp's approvals

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
What guards tool calls when the harness is omp?

## Facts
Verified 2026-10-13.
- omp approvals: `tools.approvalMode` (`always-ask`, `write`, `yolo`; schema default `yolo`) and `tools.approval.<tool>: allow | deny | prompt` (`src/tools/approval.ts`), plus built-in prompts for dangerous commands.

## Options
| Option | Pros | Cons |
|---|---|---|
| omp's approvals | No Jarvis code in omp | Named deny rules (for example `git push --force` by rule name) become prompts; the chat-only trunk becomes plan mode |
| Same, plus the guard as one omp extension | Keeps the deny rules and the chat-only trunk | The only Jarvis code inside omp |

## Recommendation
omp's approvals.

## Choice
omp's approvals are the guard: `tools.approvalMode`, per-tool policies and its built-in dangerous-command prompts, with values from the walkthrough (0396). Jarvis's permission engine, git-guard corpus, chat-only mode, approval overlay and redaction extension retire; the fixture gate ends.

## Reason
"omp + config, Jarvis code only outside omp" (chosen over keeping the guard as one omp extension).

## Consequences
- Plan section 4 drops the fixture gate; `AGENTS.md` follows.
- Amended by 0450: the guard ships as `config/guard.yml`, passed to every pane as an extra `--config`.
- Amended by 0484: boss-only tools need a per-run secret, and a merging or PR-opening `thread_finish`, `worktree_remove` and a cross-project `thread_start` wait for his Allow in the companion.
