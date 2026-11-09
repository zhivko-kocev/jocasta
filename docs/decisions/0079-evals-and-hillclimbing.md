# 0079 Evals and hillclimbing

- Status: decided
- Kind: product
- Phase/slice: 0/1.10
- Date: 2026-10-07

## Question
How does the plan take the eval-design and hillclimbing rules?

## Facts
Verified 2026-10-07.
- Anthropic, "Automating eval design and hillclimbing with Claude", 2026-09-28, https://claude.dev/blog/automating-eval-design-and-hillclimbing/: good evals mirror real use, score higher with stronger models and more effort, and keep headroom and low run-to-run variance (leftover state from an earlier trial can hand over the answer). Graders are the cheapest that fit: programmatic first; a model judge scores a rubric of checkable claims with a model other than the one under test. Hillclimb cheap, attributable surfaces (prompts, skills, tool descriptions, model and effort), one change per round; keep a change only when train and held-out test both rise; never paste failures into the prompt; within noise, don't merge. Its cost climb found Sonnet 5 at low effort scoring about the same as Opus 5.5 at about half the cost.
- The claude-api skill (Apache-2.0), https://github.com/anthropics/skills/tree/main/skills/claude-api: `shared/evals/build-eval.md` takes cases from production transcripts first, after checking for data that "can't sit in a repo", then bug reports, five to ten hand-written cases, and synthesized variations of real ones; 15 to 100 inputs; for an agent that acts on an environment, a programmatic end-state check is the primary grader; the user signs off on a graded pilot. `shared/evals/eval-hillclimb.md` checks the noise floor before round 1 (a pass rate's 95% CI half-width is about 1/sqrt(cases × reps): 25 cases × 2 reps ≈ ±14 points), splits at random stratified by tag and never by score, lets the analyzer read only train transcripts, and writes per case a `results.jsonl` row (model read from the response, usage, grade) plus one transcript per rep.
- Plan 1.10 before this record: v4 "Harness evals" (20 fixture tasks; recorded scripts in CI; a real provider at each phase end) | fewer | none. R6 and R10 already cite evals.
- His real tasks are 658 omp sessions and 86 Claude Code transcripts (appendix A); they name clients, so they can't enter the public repo (0035, principle 6).
- Real-provider runs on the Anthropic subscription bill as extra usage (0062).

## Options
| Option | Pros | Cons |
|---|---|---|
| Widen 1.10, add R15 and the eval seam | Phase 1 opens with an agenda that makes later defaults measurable; Jarvis's eval can reuse the skill's results layout, report page and hillclimb instead of building them | A bigger 1.10; real-provider runs bill extra usage |
| R15 only | Least churn | 1.10 stays at "20 tasks or fewer", too noisy to see small changes |
| Widen 1.10 and add a principle "measured defaults" | Appendix G's values become measured | Every lever slice pays for an eval run |
| Nothing | — | Evals stay one yes/no question |

## Recommendation
Widen 1.10, add R15 and the eval seam.

## Choice
Widen 1.10, add R15 and the eval seam.

## Reason
Not given.

## Consequences
- Plan 1.10: the eval question becomes six decisions: eval cases, graders, runs and results, hillclimbing, who runs a climb, and when evals run.
- Appendix H gains R15; appendix I gains the seam "the eval cases, runner and results layout"; appendix B gains the claude-api skill as a reference.
- The principle "measured defaults" was not taken; appendix G keeps its recommendations.
