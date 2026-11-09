# 0427 Ponytail kept, easy to remove

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
0421's eval said KEEP by a hair. Is Ponytail used, and how is it removed later?

## Facts
Verified 2026-10-14.
- Run `2026-10-14T07-46-50-981Z-ponytail` vs the omp baseline `2026-10-13T19-23-14-654Z-default`: 36/36 both; lines 0.89, tokens 1.09, wall time 1.91 (mostly slow model replies), cost $2.74 vs $2.40.
- The baseline ran on omp 18.10.3, the variant on 18.10.4.

## Options
| Option | Pros | Cons |
|---|---|---|
| A fresh 18.10.4 baseline, then decide | Same omp on both sides | One more paid run |
| Keep it now | The rule says KEEP | About 9% more tokens per task |
| Don't use it | Nothing changes | Gives up about 11% fewer lines |

## Recommendation
A fresh baseline first.

## Choice
Ponytail is kept. Its text ships as `config/ponytail.md` (MIT notice and source in its header); every project and thread pane starts with `--append-system-prompt <config>/ponytail.md` while that file exists. Removing it is deleting that one file (from the install, or from the repo for every build); the boss pane doesn't get it. The eval variant `ponytail` points at the same file.

## Reason
"we keep it but it is easy to remove it later if i dont see value yes?"

## Consequences
- Differs from the recommendation.
- Carries out 0421: the text is added by the service's pane command rather than by `jarvis setup`.
- Amended by 0469: `config/ponytail.md` gains four test rules.
