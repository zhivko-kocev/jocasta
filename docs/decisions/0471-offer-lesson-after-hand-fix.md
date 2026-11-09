# 0471 Jarvis offers to save a lesson after a hand-fix or a rejection

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
0459 writes a lesson only when he asks. Does Jarvis offer one when a thread's work needed his hand?

## Facts
Verified 2026-10-23.
- DHH's "fix the factory": writing code by hand is "an exceptional state… like seeing a bug in Sentry… then we fix the factory" (Rails World 2026 keynote, about 00:55, transcript https://gist.github.com/cristoslc/d17809f90a256af4d1008a1a36f9abaa; `local://omarchy-OmarchyAI.md`).
- 0459 writes lessons only on his word.

## Options
| Option | Pros | Cons |
|---|---|---|
| The boss offers once after a rejection or hand-commit | What went wrong reaches the next plan; still his decision | One more prompt |
| Only on his word, as 0459 | No prompts | Lessons are often forgotten |

## Recommendation
The boss offers once after a rejection or hand-commit.

## Choice
When he rejects a thread's plan or review, or commits by hand on a thread's branch, the boss offers once to save what was learned as a lesson for that project; nothing is saved unless he says yes.

## Reason
"Yes, offer it".

## Consequences
- Amends 0459 (lessons can start from the boss's offer; still written only after his yes).
- The offer's text is chosen by the builder; the boss manual follows.
