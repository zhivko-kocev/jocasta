# 0695 Light defaults

- Status: v1 (built 2026-11-26)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J2
- Date: 2026-11-26
- Amends: 0442 (no default feeds), 0470 (the weekly review is opt-in per project), 0616 and 0619 (the advisor's cadence), 0299 and 0405 (which day events reach the boss)

## Question
Out of the box Jocasta polls three news feeds nobody listed, runs a big-picture review of every project each week, has the advisor review every third update and interrupt for concerns, and gives the boss a model turn for every reminder, meeting, low disk, red pipeline, review request and quiet-hours release, though `today` already shows each. Which of these stay on by default?

## Facts
- `src/shared/pollers/settings.rs` filled `pollers.news.feeds` with Hacker News, LWN and gaming when unset (0442); the poller runs only with feeds (`day.rs`).
- The weekly review ran for every project on the minute tick (0470); `config/headless.yml` already runs it without the advisor (0619).
- Jocasta's measurements (`docs/research/state-of-the-art.md`): the advisor was 24 % of cost; all six of its blocker notes were acted on, none of its 15 nits; reviewing at agent end cut its cost 27 %. omp 19.2.1 has `advisor.reviewMode: turn|agent-end` and `advisor.continueSeverity: nit|concern|blocker`.
- Every delivered day event (`day.rs` `deliver`) and every quiet-hours release (`deliver_held`) was an `[event]` line, so a boss turn, besides its `today` row and notification.

## Options
| Option | Pros | Cons |
|---|---|---|
| Light defaults: news without feeds is off, weekly review per listed project, advisor at agent end for blockers, the boss woken only by thread outcomes | Less spend and fewer interruptions; each is one key to turn back on | A new user sees no headlines until he lists feeds |
| Keep today's defaults | Nothing to configure | Pays for work nobody asked for |

## Recommendation
Light defaults.

## Choice
His answers of 2026-11-26 and the approved plan:
- `pollers.news` is off until `pollers.news.feeds` lists feeds; there are no default feeds.
- New `weekly.projects` in `jocasta.yaml`: a list of project names (default empty). The minute tick starts the weekly big-picture review only for a listed project; the boss's `big_picture_review` tool still runs one on request, and the weekly brief is built as before.
- `config/omp/config.yml`: `advisor.reviewMode: agent-end` and `advisor.continueSeverity: blocker`; `advisor.reviewInterval` is dropped.
- Of the day's events only the weekly review's result is still an `[event]` line for the boss. Reminders, meeting soon, disk low, red pipeline, review requested and the one "Still open" repeat keep their `today` row and notification and no longer wake the boss. Thread lines in quiet hours are no longer held for the boss: they go into today's activity, and nothing is released when quiet hours end. A thread's plan ready no longer wakes the boss (its `today` row, notification and Allow stay).
- Thread outcomes keep waking the boss as before: a review is ready, stuck, over budget, the $50 spend warning, a waiting start that became blocked, awaiting you, finished its turn, merged, PR open and lesson offers. (His ruling of 2026-11-26 on the plan's list: change exactly what J2 names, nothing more.)

## Reason
Spend less, achieve more: the boss pays a model turn only for what needs its judgement, and `today` already shows the rest.

## Consequences
- `config/boss-manual.md` no longer says to relay reminders, meetings, disk, pipelines and review requests.
- Thread lines that happened in quiet hours reach the boss only through `today`; open `held` rows left by an earlier version are resolved on the next tick.
- An existing install sees the advisor change as a snapshot change in `jocasta setup`.
