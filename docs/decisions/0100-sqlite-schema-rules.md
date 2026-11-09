# 0100 SQLite schema rules

- Status: decided
- Kind: product
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
Which rules do Jarvis's SQLite schemas follow?

## Facts
Verified 2026-10-07 in a Bun 1.4.2 binary compiled on Windows: a `STRICT` table refused text in an `INTEGER` column, and `PRAGMA user_version` stored the value set.
- Section 7: schema changes are numbered migrations tracked in `PRAGMA user_version`.
- Asked as a tick-list (0089); every item was ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| `STRICT` tables; RFC3339 UTC text timestamps (`2026-10-07T14:03:22.123Z`); numbered migrations, each in a transaction, with the version in `PRAGMA user_version` per DB file | Typed columns; timestamps sort as text and read in any tool; one version per file | Migrations are written per file |
| Driver defaults | Nothing to write | Loose typing; no migration discipline |

## Recommendation
The three rules.

## Choice
`STRICT` tables; RFC3339 UTC text timestamps with milliseconds; numbered migrations, each in a transaction, tracked in `PRAGMA user_version` per DB file.

## Reason
Not given.

## Consequences
- Appendix I's migration ledger numbers migrations per DB file (0096).
