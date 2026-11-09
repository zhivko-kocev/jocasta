# 0516 The agent operates the VM benchmarks' manual steps, logged; H8's plans are approved unread

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-01

## Question
H2, H6 and H8 run in the VM's sandbox Jocasta and have steps their scripts call his: his Allow box on the bench's own `thread_start` calls, closing the threads between runs and pressing Enter, and approving H8's plans. 0482 doesn't say who does them. Who operates them, and how are H8's plans approved?

## Facts
Verified 2026-11-01.
- `thread_start` asks for his Allow when no project pane is focused (0492); `thread_finish` and `worktree_remove` ask him every time (0484).
- The bench never closes threads or removes worktrees (`bench/README.md`); H6 and H8 print "close … threads, then press Enter" and read a line between runs; H2 starts its next thread after each toast, so its 30 asking threads pile up (an idle omp holds about 470 MB private in the VM, H6's competitor row, against 8 GB).
- Closing a thread's workspace in herdr is his own act and needs no box; Jocasta ends the pane's leftovers (0490), and its folder stays on disk as a leftover worktree.
- H8's threads open in plan mode; the task benchmark's A2 approves its own plans unread (`--plan-yolo`, `evals/variants/a2.yaml`).
- The clean-machine test pressed Allow in the VM through `allow-box.ps1` (0481).

## Options
Who operates:

| Option | Pros | Cons |
|---|---|---|
| Agent, logged | Runs now, unattended; as the clean-machine test did | His gate is exercised by the agent, not by him |
| Wait for him | His gate stays his | The full run waits too: setup's files must leave tester's omp folder first |
| Full run first | No waiting on him for the full run | H2, H6 and H8 later, with Jocasta reinstalled in the sandbox |

H8's plans:

| Option | Pros | Cons |
|---|---|---|
| Approved unread, as A2 | Reproducible; a lower bound on isolation | The plan gate's help isn't measured |
| The agent reads each against its brief | Closer to a reviewed plan | The agent's judgment enters the numbers |
| He approves each plan | Faithful to "he approves each plan" | H8 waits for him |

## Recommendation
Agent, logged; approved unread, as A2.

## Choice
- In the VM's sandbox only, the agent acts as its user for the benchmarks' own steps: it presses Allow only on boxes that name a bench thread, closes the bench's thread workspaces in herdr between runs (never `thread_finish` or `worktree_remove`), and presses Enter. Every action is logged with its time in the run's notes, and the thesis says these runs were agent-operated.
- H8's plans are approved unread as they appear, as A2's `--plan-yolo` does: H8 measures isolation without the plan gate's help.

## Reason
"Agent, logged"; "Approved unread, as A2".

## Consequences
- An operator helper beside the VM's scripts does these steps and writes its log into each run's results folder.
- H2's threads are closed once their trial's toast is in, so the VM doesn't page during the run; the notes say so.
- Amends 0482's procedure before the run, as 0482 asks of any change.
- Amended by 0525: the agent also cancels the VM's Firewall prompt for Node.js, a step outside the benchmarks' own, after noting the program it names.
