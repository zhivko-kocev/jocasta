# 0668 The dock: a five-line news box, and each thread's diff, check, PR and stuck state

- Status: v1
- Kind: product, ux
- Phase/slice: HUD and orchestration round, after the fork build
- Date: 2026-11-23

## Question
He asked: "we should put more functionalities in the right hud, the news can be scrollable and take less space and we do more there". What does `today` draw, and how much room does the news get?

## Facts
Verified 2026-11-23 (`src/frontier/today/view.rs`, `protocol.rs`):
- The dock draws its header, NEEDS YOU, UNDERWAY, LANDED TODAY, CALENDAR, then NEXT and NEWS in the lines left. NEWS takes every spare line, with no cap, and does not scroll.
- A thread row shows its title, one line, state, cost and context. The service already knows each thread's worktree and branch, its checks' results by git tree (the check cache), its PR when sent as one, and the time of its session file's last write.
- Research of 2026-11-23 (omp and pi issues, 5,350 pi packages, orchestrator dashboards): users ask first for "needs you", then cost and quota, then each agent's live state; "an idle agent looks identical to a finished one" is the most named complaint. Notes in `docs/research/hud-research.md`.

## Options
News: **a fixed five-line box, scrolling** (recommended) · one collapsed line that opens the box · the box hidden while NEEDS YOU has rows.
Additions (each a tick): a thread's diff against trunk and last check; a stuck flag; PR and CI state; a spend trend; the service's last events.

## Recommendation
The five-line box; diff and check, stuck, PR and CI, spend trend.

## Choice
His answers of 2026-11-23:
- **News:** a box of five lines, one headline per line, newest first across the feeds; the wheel, or the arrows once it is picked, scroll it; Enter or a click opens the link.
- **Thread rows** show the diff of the thread's branch against trunk (`+84 −12`) and its last check (passed or failed), from git and the check cache.
- **Stuck:** a thread whose state is working and whose session file hasn't been written for 10 minutes is drawn as stuck and gets a NEEDS YOU row.
- **PR and CI:** a thread sent as a PR shows its PR number and CI state, from `gh`, cached; a red pipeline keeps its NEEDS YOU row.
- **Spend:** the header's spend shows today against the last seven days' average, and NEXT or the spend line names today's costliest thread.
- The service's last events stay undrawn.

## Reason
His dock is the one place he looks; what it adds must come from data the service already reads, with no model call (0599).

## Consequences
- Amends 0555 (the dock's contents) and 0442 (the news' room).
- No new model calls; `gh` is already used by the pipeline poller.
