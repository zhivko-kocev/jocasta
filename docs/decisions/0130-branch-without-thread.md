# 0130 A branch with no thread

- Status: decided
- Kind: product
- Phase/slice: 1/1.5
- Date: 2026-10-07

## Question
What happens when `jarvis` opens interactively on a non-trunk branch that no workspace thread owns?

## Facts
- Every workspace thread has a title and a brief (0123); an existing branch binds as is (0121).
- 1.7's first item asks the same for headless runs.
- The question was missing from the agenda and was asked under 0020.

## Options
| Option | Pros | Cons |
|---|---|---|
| Offer to start a workspace thread bound to the branch, title from the branch, brief asked; declining opens the project thread, chat-only | Threads are never created silently | One prompt the first time |
| Create one silently | No friction | Breaks the brief rule; every stray branch becomes a thread |
| The project thread only | No new thread | No work on that branch until a thread is created by command |

## Recommendation
Offer a thread, else the project thread.

## Choice
Free text: "the first one the recomended one i love the recomendations but also jarvis could speak this right and i can answer to him?" Jarvis offers to start a workspace thread bound to the branch, with the title from the branch and the brief asked; declining opens the project thread, chat-only. Once voice lands, Jarvis can speak the offer and he can answer by voice.

## Reason
"the first one the recomended one i love the recomendations but also jarvis could speak this right and i can answer to him?"

## Consequences
- Prompt, as previewed: `Branch feature/spike-cache has no workspace thread. Start one? Title: Spike cache  Brief: _  [start] [open project thread (chat only)]`.
- Voice: 4.8 (dictation) and 7.4 (speech) decide which prompts are spoken and answered by voice; this offer is one he named.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
