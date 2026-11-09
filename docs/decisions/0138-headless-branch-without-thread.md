# 0138 Headless run on a branch with no thread

- Status: decided
- Kind: product
- Phase/slice: 1/1.7
- Date: 2026-10-07

## Question
What does `jarvis -p` do on a non-trunk branch that no workspace thread owns?

## Facts
- Interactively, Jarvis offers a thread (0130); a headless run can't ask.
- Every workspace thread has a title and a brief (0123); a headless run carries its prompt.
- What changed (0004): the brief rule (0123) came after the plan's options; the prompt fills it.

## Options
| Option | Pros | Cons |
|---|---|---|
| Create a workspace thread titled from the branch, with the prompt as its brief | No ceremony; the brief rule holds | Scripts on stray branches create threads |
| Refuse with a hint (v4) | No unasked threads | One more step per scripted run |
| Run in the project thread | No new thread | The chat-only project thread can't edit (0117) |

## Recommendation
Create a thread, prompt as brief.

## Choice
Jarvis creates a workspace thread bound to the branch, titled from the branch name without its prefix, with the `-p` prompt as its brief, runs in it, and names the new thread on stderr.

## Reason
Not given.

## Consequences
- Message, as previewed: `jarvis: started workspace thread Spike cache (t-3a7c9e10) on feature/spike-cache`.
- Amended by 0387: the harness command is `omp` with its config in `~/.omp/agent`; Jarvis-only files stay in `~/.jarvis/agent`.
