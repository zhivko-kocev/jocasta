# 0099 SQLite connection settings

- Status: decided
- Kind: product
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
How does Jarvis open each SQLite connection?

## Facts
Verified 2026-10-07 in a Bun 1.4.2 binary compiled on Windows: WAL took effect, and foreign keys were enforced once switched on.
- Lesson L66: set `busy_timeout` before switching to WAL, because the switch takes a lock; WAL with `synchronous = NORMAL` survives process crashes and may lose the last commits on power loss.
- Pi Durable's SQLite storage uses WAL with `synchronous = NORMAL` (`packages/durable/README.md:524`).
- SQLite leaves foreign keys off per connection unless switched on.
- Asked as a tick-list (0089); every item was ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| `busy_timeout = 5000` set first, then `journal_mode = WAL`, `synchronous = NORMAL`, `foreign_keys = ON` | Readers never block the writer; waits instead of failing on a brief lock; bad references fail loudly | The last commits can be lost on power loss |
| Driver defaults | Nothing to write | Rollback journal blocks readers; foreign keys unenforced |

## Recommendation
The four settings.

## Choice
Every connection runs, in order: `busy_timeout = 5000`, `journal_mode = WAL`, `synchronous = NORMAL`, `foreign_keys = ON`.

## Reason
Not given.

## Consequences
- Applies to every Jarvis DB file (0096).
