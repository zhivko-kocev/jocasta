# 0256 Running threads when Frontier quits

- Status: decided
- Kind: product
- Phase/slice: 4/4.1
- Date: 2026-10-08

## Question
What happens to running threads when Frontier quits or the PC restarts?

## Facts
- Each thread's Pi session is saved as it runs (0131), so nothing is lost.

## Options
| Option | Pros | Cons |
|---|---|---|
| Marked interrupted; the boss lists them on the next start; each continues on his word | No surprise restarts | He restarts them |
| Auto-continue | Picks up by itself | Surprise work |
| Cancel | Simplest | Work stops for good |

## Recommendation
Interrupted; continue on his word.

## Choice
Now (0357): threads run in the background host, so closing or restarting Frontier leaves them running and a new Frontier reattaches. Running threads are marked `interrupted` only when the host stops: Quit's "Stop threads too", or a PC restart. On the next start the boss lists them, and each continues when he says so.

Originally: Running threads are marked `interrupted`; on the next start the boss lists them, and each continues when he says so.

## Reason
Not given.

## Consequences
- `interrupted` joins 4.5's thread states.
- Slice 4.1 is decided.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
