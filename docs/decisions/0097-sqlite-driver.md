# 0097 SQLite driver

- Status: decided
- Kind: product
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
Which SQLite driver does Jarvis use?

## Facts
Verified 2026-10-07 in a Bun 1.4.2 binary compiled on Windows, in a temp dir.
- `bun:sqlite` and `node:sqlite` (`DatabaseSync`) both run SQLite 3.53.2 with WAL, FTS5 and `STRICT` tables; `bun:sqlite` enforced foreign keys and stored `PRAGMA user_version`.
- `@earendil-works/pi-durable` 1.0.4 (npm, 2026-10-05) is experimental, stores its own conversations, has one owner process, and is not in the pinned set.

## Options
| Option | Pros | Cons |
|---|---|---|
| `bun:sqlite` | Native to Jarvis's runtime and compiler (0036); sync API; proven here by L66, 0078 and today's probe | Code tied to Bun |
| `node:sqlite` | Node-portable API | Under Bun, a compatibility layer over the same engine; Jarvis ships only as a Bun binary |
| Pi Durable's SQLite storage | Pi's own durable store | Experimental API; one owner process; not a general driver; a new dependency |

## Recommendation
`bun:sqlite`.

## Choice
`bun:sqlite`.

## Reason
Not given.

## Consequences
- Appendix B's SQLite row is decided: `bun:sqlite`.
