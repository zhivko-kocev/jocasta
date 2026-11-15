# 0609 The code-quality standard: the five sources, Ousterhout where they disagree

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-14

## Question
He added to the tuning program: "on top of all this i still want this environment to produce good quality maybe above average clean state of art code", judged against Ousterhout's CS190 notes on modular design, Clean Code, Refactoring, Design Patterns and Designing Data-Intensive Applications. Those sources disagree in places. Which reading is Jocasta's standard?

## Facts
Verified 2026-11-14 (reports in `D:/jocasta-spikes/sota/reports/`, outside the repo; the code-quality section of `docs/research/state-of-the-art.md` carries them).
- **What was read that day:**
  - Ousterhout's CS190 notes (modular design, complexity, comments, exceptions, testing): in full.
  - The Ousterhout and Martin debate: its parts on method length, comments and test-first.
  - Clean Code: the table of contents with the names of its smells and heuristics. Refactoring: the catalog index and the smells page. Design Patterns and Designing Data-Intensive Applications: their contents pages.
  - The books' chapters were not re-read: rules cited to a chapter are from knowledge and marked so.
- **A rubric of 60 rules** came out of the five sources. 47 can be checked by a program (a Biome rule, a tsc option, a script or a test); 13 need a reader's judgement.
- **Where the sources disagree:**
  - **Function length.** Martin: very small functions. Ousterhout: depth matters more than length; split only when the parts are clean.
  - **Comments.** Martin: a comment is a failure to express in code. Ousterhout: a comment on every interface, for its caller.
  - **Class size.** Martin: small, one reason to change. Ousterhout: 200 to 2,000 lines is fine behind a small interface.
  - **Test-first.** Martin: tests first, in short cycles. Ousterhout: tests with the change, and one for every bug.
  - **Names.** Martin: a long descriptive name beats a comment. Ousterhout: a short name, the comment carries the contract.
- **The repo as it stands** (`src/`: 30,730 lines of strict TypeScript, measured by scripts over its syntax tree):
  - named functions: median 7 lines, 90th percentile 28; 13 over 100 lines; a 30-line rule would flag 149;
  - cognitive complexity over 25 in 36 functions, over 40 in 11;
  - 96.8% of functions carry a doc comment, 62.7% of classes, 67.9% of types; no commented-out code;
  - 21 boolean parameters tested in a branch, 64 forwarding functions, 2 import cycles, 289 `as` casts, no `any`.
- **Published studies** (read levels in the research document):
  - Maintainers would merge about 24 points fewer test-passing patches than the tests pass; for newer Claude models the rejections were mostly code quality (METR, 296 patches).
  - Test-first instructions raised regressions in the one study that measured them, on small models.
- **The lesson `tdd`** already ships as a per-project opt-in (0459).

## Options
| Option | Pros | Cons |
|---|---|---|
| Ousterhout, with ceilings for outliers | It is how the repo is already written; a 30-line rule would push agents toward shallow helpers | No hard small-function rule |
| Martin: small functions as a rule, comments as a last resort, test-first as a house rule | Simple to check | Against the repo's interface comments; the test-first evidence above |
| No written position | Nothing to maintain | Each check then has no stated basis |

## Recommendation
Ousterhout, with ceilings for outliers.

## Choice
- **The five sources are the standard** for Jocasta's own code and for the code its panes write.
- **Where they disagree, Ousterhout's reading holds:** deep modules, an interface comment on each exported item, a function split only when the parts are clean.
- **Ceilings catch the outliers** (0610). They are limits, never targets.
- **Test-first stays a per-project lesson**, as shipped.
- **The rules a program can check are held by programs, not by text in every session** (0610). The rules that need a reader go to the review (0611).
- **One rule set for new work and for maintenance.** Ponytail (0612) says how much to write: reuse first, the smallest complete change, nothing unasked. The standard says what shape the written code has. A fix gets both, and no thread is labelled as maintenance.

## Reason
His answer: "yes i love the recomended and i agree that somewhere they disagree but they have a lot of common things also". In the same answer he asked "for maintaining i guess ponytail is great or?"; asked back with the two options (one rule set, or a maintenance kind of thread with Ponytail only and no design section), he chose one rule set.

## Consequences
- The rubric and its placement are in the research document; 0610, 0611 and 0612 carry it out.
- A quality scorecard is part of the tuning program's before and after measurement.
- The books' chapters stay cited from knowledge until they are re-read.
