# 0611 The review gets a design section; the weekly review gets the same rubric

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-14

## Question
13 of the standard's rules (0609) need a reader's judgement, and a design reading costs tokens. Where is design looked at?

## Facts
Verified 2026-11-14 (the code-quality section of `docs/research/state-of-the-art.md`).
- **Nothing looks at design before a merge.** The per-thread review (`reviewPrompt` in `src/frontier/server/hunk.ts`) runs omp's `reviewer` agent on the thread's diff and tells it to report only material findings. The weekly review (0583, `config/agents/drift.md`) looks for design drift, after the merge.
- **In 0573's review, 42 of 242 findings were duplication, dead code and structure**: kinds the per-thread review is told not to raise.
- **A reviewer run costs $5.10 on average** (13 runs in the window, $66.36).
- **No study measures a reviewer given a design rubric.** Anthropic's own guidance warns that fixing every review finding adds abstraction layers and tests for cases that cannot happen.
- **Notes block by priority:** a review's notes marked P0 or P1 count as blocking (0453).

## Options
| Option | Pros | Cons |
|---|---|---|
| A design section in the review that already runs, and the same rubric in the weekly review | Design is read before the merge at a small added cost | Unmeasured; needs a cap against inflated diffs |
| The weekly review only | Nothing added per thread | Findings arrive after the merge |
| A second, separate design reviewer per thread | A fresh reader | About $5 more per thread; the largest risk of inflated diffs |
| None; machine checks only | No tokens | The 13 rules are checked by nobody |

## Recommendation
A design section in the review that already runs, and the same rubric in the weekly review.

## Choice
- **The per-thread review's reviewer gets the standard's judgement rules**, about 600 words, paid only in a review.
- **It reports at most three design findings**, as notes that do not block.
- **It names code the change could delete.**
- **The weekly review's drift agent gets the same rubric.**

## Reason
His answer: "same-run".

## Consequences
- Amends 0453 (the review prompt) and 0583 (the drift agent's text).
- The cap of three and the non-blocking notes are the guard against the reviewer inflating diffs. Diff size per thread is in the scorecard (0610).
- The rubric's text is a shipped prompt: written by the builders from the research document's rule list and shown to him as built (0363).
