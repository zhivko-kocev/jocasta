# 0123 Creating workspace threads

- Status: decided
- Kind: product
- Phase/slice: 1/1.4
- Date: 2026-10-07

## Question
Who can start a workspace thread, and is a brief required?

## Facts
- The boss arrives in phase 4 (4.5 dispatch); `ask` overlays in 3.6; how commands reach the CLI is 1.6; appendix I claims `thread new`.
- Principle 2: no model-judged automatic behavior where a fixed rule works.

## Options
| Option | Pros | Cons |
|---|---|---|
| He starts one by command or from the project thread, where the agent may propose one that he confirms; the boss from phase 4; title and brief required | Every thread knows why it exists | One more field each time |
| Title only | Fastest | Report-back has nothing to check against |
| The agent may start one freely | Hands-off | Threads appear unasked |

## Recommendation
He or the boss; title and brief required.

## Choice
Free text: "me or the boss (recomended suggestion) + boss if he sees need". Read as: the recommended option, and in addition the boss may start a workspace thread on its own when it sees a need. Every workspace thread has a title and a brief; the brief is its first message.

## Reason
"me or the boss (recomended suggestion) + boss if he sees need"

## Consequences
- He starts threads with `jarvis thread new` or from the project thread, where the agent may propose one (title and brief) that he confirms.
- From phase 4 the boss starts threads when he asks and when it sees a need; 4.5 decides the rules for the second case.
- The boss starting threads unasked bends principle 2; he chose it.
- Command preview: `jarvis thread new "Add retry" --brief "…"`; flag names are decided with the `thread` commands.
- Amended by 0388: a thread is a herdr tab on a worktree running `omp --continue`; Jarvis's thread records, `report` and harness exit codes retire.
