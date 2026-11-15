# 0624 omp's install is outside the package-age rule; 0617 is reviewed again

- Status: v1
- Kind: process
- Phase/slice: program
- Date: 2026-11-15

## Question
The bump to omp 18.20.2 (0623) installs bun's global omp package at 18.20.2, which was published on 2026-11-14. 0614 says a package must be 7 days old to install. Does that rule hold for omp's own install? And is the 0617 build reviewed again on the new model?

## Facts
Verified 2026-11-15.
- **0614's rule** is `minimumReleaseAge` in Jocasta's `bunfig.toml`, written for the packages Jocasta itself depends on.
- **omp on his PC is already 18.20.2:** its standalone binary replaced bun's shim at 00:50Z. He ran that update himself (his answer, below). Bun's global package, whose source H1's guard tests load, is still 18.20.0.
- **omp's own updater** installs through bun with `--minimum-release-age=0` (`cli/update-cli.ts`, `buildBunInstallArgs`).
- **0623 approved the install;** the package's age was not part of that question.
- **The 0617 build** (a failed check reaching a project pane's agent at its turn end) was built before he switched the model to Opus 5.5.

## Options
| omp install | For | Against |
|---|---|---|
| Install now; the rule does not cover omp | H1's guard tests run on the omp he uses | a 1-day-old package, like the binary already running |
| Hold the bump until 2026-11-21 | the rule holds for every package | H1 and the guard stay checked on 18.20.0 while the panes run 18.20.2 |

The re-review was offered as a tick, preselected.

## Recommendation
Install now; re-review 0617.

## Choice
- **omp's install is outside the 7-day rule.** It is his daily tool and follows the version he runs (0383). The rule still holds for every package Jocasta depends on.
- **The 0617 build is reviewed again on Opus 5.5** after the builds.
- **Fact for 0623:** he started the 18.20.2 update himself.

## Reason
His answers: `install-now`, the re-review ticked, and on what started the update: "me".

## Consequences
- Amends 0614 (the rule's scope) and 0623 (its fact on how the update started).
- The bump is the first build slice. He said to start on 2026-11-15 ("i switched to opus you can start").
