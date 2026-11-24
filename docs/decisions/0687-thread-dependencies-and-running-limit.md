# 0687 Thread dependencies (`after`) and a running limit of 4

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
How do threads wait for one another and for room?

## Facts
- One working omp is 405 MB; `thread_start` creates a worktree and pane at once today.
- `thread_send`'s resolver turns a name into a thread.

## Options
| Option | Pros | Cons |
|---|---|---|
| Boss starts each thread by hand when ready | Nothing to build | Uses boss turns and attention |
| `after` plus a running limit, queued and started by code | No model turn; branch from trunk as it then is | A pending store |

## Recommendation
Queue by code.

## Choice
His answer of 2026-11-24:
- Setting `threads.maxRunning` (integer, default 4, 0 = no limit); a thread runs while its omp is alive and not asleep.
- `thread_start` takes `after` (thread references, resolved like `thread_send`'s; unknown is an error). With a non-empty `after` or the limit reached nothing is created: a `Pending {id, project_id, title, args, after: [{thread, met}], queued_at_ms, blocked}` is stored in `<data>/projects/<pid>/pending.json` (`pending.rs`) and the tool answers `Waiting: <title> starts after <names>.` or `Queued: <title> (<n> running); it starts when one finishes or sleeps.` A pending id is a valid `after` reference, so cycles cannot be written.
- An `after` entry is `met` when its thread's work is in trunk (end of a `Mode::Merge` finish, or its PR read as merged). A parent stopped or removed unmet sets `blocked` and raises `<title> waits on <parent>, which was stopped`; a blocked item never starts by itself.
- `Pending::tick` on the refresh tick and after every finish, sleep and stop: under the limit, start the oldest unblocked item whose `after` are all met through the normal `thread_start` path; a failed start keeps the item, sets `blocked` and raises the error.
- `thread_stop` on a pending id removes it; `thread_status` and `today` list `waiting: <title> · after <names>`, `queued: <title>`, `blocked: <title>`. Journal kinds `queued {after}` and `started_from_queue`.

## Reason
Work starts by itself when its turn comes.

## Consequences
The morning brief's work graph draws these edges (0680).
