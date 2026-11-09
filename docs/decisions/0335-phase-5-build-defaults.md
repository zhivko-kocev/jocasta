# 0335 Phase 5 build defaults

- Status: decided
- Kind: product
- Phase/slice: 5/5.1–5.8
- Date: 2026-10-08

## Question
Which visible choices does the phase 5 build use?

## Facts
- Appendix I claims a `memory` command for phase 5. 0279: agents and the evaluator propose, he confirms one at a time through the boss. 0281: the importer is a one-off he starts; the nightly run happens inside Frontier's server. 0278: at most 200 lines of `memory.md` load at thread start. Model roles arrive in 6.2 (0283).
- Each item was explained twice at his request.

## Options
| Option | Pros | Cons |
|---|---|---|
| The six defaults below | Minimal; every write still waits for his word | — |
| Ask each as its own question | — | Slower |

## Recommendation
The six defaults.

## Choice
- `jarvis memory import`: reads his omp sessions and Claude Code transcripts once and queues proposals; no other `memory` verbs.
- A `remember(text, scope: project|personal)` tool in every thread and the boss queues a proposal; nothing is written until he accepts.
- Proposals show in the boss chat one at a time as a card with the line and its project: accept, edit, reject.
- `memory.md` is plain markdown, one `- ` line per entry, newest last; only the first 200 lines load, and the nightly run proposes merges past that.
- The close evaluator runs on the default model at thinking `low` until 6.2's `jarvis/cheap`, then on that role.
- The nightly run is at 03:00 local while Frontier runs; a missed night runs at the next start.

## Reason
He ticked all six, then asked for them explained again and ticked all six again.

## Consequences
- Appendix I: `jarvis memory import` and the `remember` tool.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
