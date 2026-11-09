# 0490 A thread pane he closes in herdr ends its own leftovers

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
0486 ends a pane's process tree only when Jocasta closes or stops the pane. What happens when he closes a thread's pane or workspace in herdr himself?

## Facts
Verified 2026-10-24 in the campaign's security re-test (0481), on a sandbox build.
- After `herdr workspace close` on a thread, omp (the pane's process) ended, but three processes it had started kept running with the worktree as their folder: a background `bun -e` started from the pane (its parent the dead omp), and omp's two mnemopi workers (`__omp_worker_mnemopi_embed_server`, parent the dead omp, and `__omp_worker_mnemopi_embed`, parent the first worker).
- `worktree_remove` then refused, naming the three, as 0486's check decides; they had to be ended by hand.
- On Windows a process keeps its parent's process id after the parent ends, so a dead omp's leftovers are found by that id; `processTree(roots)` (`src/frontier/server/proctree.ts`, 0486) already includes children of a dead root.
- herdr's `pane.process_info` gives each pane's `shell_pid`, omp itself in Jocasta's panes (0486).
- The service already notices a thread pane that is gone (0405: the boss hears it once, and the worktree and branch stay).

## Options
| Option | Pros | Cons |
|---|---|---|
| Jocasta ends that pane's own leftovers | No orphans after any close; the worktree can be removed; found by the remembered process id, so a process he started in the worktree from another terminal is never touched | Jocasta keeps each thread pane's omp process id; a process that fully detached from omp can still escape |
| `worktree_remove` offers to end them | Nothing ends without his Allow | The memory workers keep running, and using memory, until he removes the worktree |
| Leave it as 0486 says | No new behavior | Orphans pile up after every close he makes in herdr |

## Recommendation
Jocasta ends that pane's own leftovers.

## Choice
When a thread pane Jocasta started is gone and Jocasta didn't close it, Jocasta ends every process descended from that pane's omp, found through the omp process id it remembered for the pane (and that process's start time, so a reused id never counts), never by folder. The worktree and branch stay (0405); 0486's check before removing a worktree is unchanged.

## Reason
"Jocasta ends that pane's own leftovers".

## Consequences
- Amends 0486 (closes he makes in herdr also end the pane's own processes).
- Plan appendix I's process-tree paragraph names the remembered process id and when it is used.
