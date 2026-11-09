# 0307 Harness prunes

- Status: decided
- Kind: product
- Phase/slice: 1.4–1.12, 2.1–2.4, 6.9 (0229)
- Date: 2026-10-08

## Question
Which harness features leave v1?

## Facts
- Pi already covers: the footer shows the latest cache hit rate and window percentage, and `/session` keeps totals (0173, 0174); Pi has its own per-folder trust (0188); hashline edits fail without a current read's tags (0146).
- No use in his history (appendix A): dry-run previews, history-aware rules, the dormant label, renamed-branch following, fabricated-result aborts. ast-grep: 6 uses; Pi 1.1.0 has no ast-grep (its `dist`, `docs` and README searched 2026-10-08); omp ships native `ast_grep` and `ast_edit` tools (`docs/tools/ast-grep.md`); `sg` isn't installed.
- With `yolo` by default (0200), unmatched calls already run, so "always" grants answer almost nothing (0222).
- He asked for the unticked items to be explained, then whether Pi uses ast-grep.

## Options
| Option | Pros | Cons |
|---|---|---|
| Prune all eleven listed | Leanest | — |
| Keep some | — | — |

## Recommendation
All eleven.

## Choice
- Dropped: the cost status segment (0173), the context gauge (0174), blocking Pi's `/new`, `/resume` and `/fork` (0132), "always" grants and `jarvis guard grants` (0222), the dormant label (0116), following a renamed branch through the reflog (0128), edit-requires-read stamps (0148), Jarvis's project trust with file hashes and its prompt and command (0219, 0221); Pi's own trust stays.
- After v1: the dry-run preview (0223), aborting turns with tool-result markup (0183), history-aware rules (0205), ast-grep (0290).

## Reason
"explain the unticked"; on history-aware rules: "maybe we will achieve with the memory management so we can prune this"; on ast-grep: "could it be useful" and "is pi using this?"

## Consequences
- Amends 0116, 0128, 0132, 0148, 0173, 0174, 0183, 0205, 0219, 0220, 0221, 0222, 0223, 0224 (no "always" choice) and 0290 in place.
- The approval overlay offers yes once, yes for the run, no, and no with a note.
- The agent can still run `sg` through bash if he installs it.
