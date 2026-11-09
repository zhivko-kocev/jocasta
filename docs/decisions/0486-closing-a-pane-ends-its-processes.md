# 0486 Closing a pane ends everything it started

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
Processes a pane's omp started can outlive the pane and hold its worktree. Is that fixed in this round?

## Facts
Verified 2026-10-24.
- Observed 2026-10-23 in the sandbox (CHANGELOG "Research round"): after the companion's "Stop everything" and then `herdr session delete jarvis-v1`, removing the sandbox failed because the thread worktree was still in use.
- Two leftover processes had the worktree as their folder: one of omp's `__omp_worker_*` Bun workers (the mnemopi embedding worker) and a `tsc --lsp --stdio` language server; they held it until stopped by hand.
- 0447 turns mnemopi on in every pane.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fix it in this round | Worktree removal and "Stop everything" work in v1; no leftover processes | One more change before the campaign |
| After v1 | Nothing new now | Worktrees stay locked after a stop until he kills processes by hand |

## Recommendation
Fix it in this round.

## Choice
- When Jocasta closes or stops a pane (thread finish, worktree removal, Stop everything, Keep's later stop), it ends the pane's whole process tree: omp and its workers (memory embedding, tiny model, language servers, MCP servers). On Windows this uses a job object or `taskkill /T`; on POSIX, the process group.
- Before removing a worktree, Jocasta checks that no process still has it as its folder.

## Reason
"Fix it in this round".

## Consequences
- 0481's reliability checks cover it (kill a pane mid-turn, Stop everything, then remove the worktree).
- Amended by 0490: a thread pane he closes in herdr himself also ends its own leftovers, found through the omp process id Jocasta remembered for the pane.
- Note (2026-11-09, 0573's review, SRV-A-1/SEC-LOC-1): process trees are walked by start-checked parent links: a process counts as a child only when it started at or after its parent (`src/shared/proctree.ts`, `endProcesses`). `taskkill /T` isn't used: it follows raw parent pids, and stale Windows parent pids (a long-dead parent's pid given to a new process) had reached unrelated apps. On POSIX a found group leader's whole group still ends.
