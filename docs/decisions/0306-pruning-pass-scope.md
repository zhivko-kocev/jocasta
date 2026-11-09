# 0306 Pruning pass scope

- Status: decided
- Kind: process
- Phase/slice: all (0229)
- Date: 2026-10-08

## Question
Which features does the pruning pass re-judge?

## Facts
- 0229: after the grilling pass, every v1 feature from 0090 on is re-judged; one stays in v1 only for a benchmark gain, daily use, or safety he asked for.
- An inventory of 91 v1 features put all of phases 5, 7 and 8 and the dictation spike after v1 on a strict reading. 0229's criteria don't cover non-safety features he asked for.
- He first chose a strict row-by-row pass, then cancelled the first tick-list and narrowed it (Reason).

## Options
| Option | Pros | Cons |
|---|---|---|
| Strict, row by row, all 91 | Follows 0229 to the letter | Re-litigates what he chose that day |
| His explicit v1 picks stay; prune the rest | Respects his asks | About 45 rows still judged |
| Two tiers: a strict v1 core, his picks as v1.1 built right after | Benchmarks measure a lean core | Two release lines |
| Only harness features Pi already covers or nobody uses (his free text) | Short; keeps his Jarvis features | — |

## Recommendation
Two tiers.

## Choice
The pass re-judges only harness features (Pi plus extensions) that Pi already covers or that show no use. Jarvis's own features (Frontier, the boss, memory, canvas, voice, day-and-night life) stay as decided.

## Reason
"dont review everyting i am very good with jarvis part. suggest only prune for some PI ffeatures if needed - memory part is excelent. maybe something except that"

## Consequences
- Narrows 0229's pass; 0229 is amended in place.
- The result is 0307.
