# 0090 Slice 1.1 agenda

- Status: decided
- Kind: process
- Phase/slice: 1/1.1
- Date: 2026-10-07

## Question
Which decisions does slice 1.1 (data dir and SQLite) grill, now that its v4 §D and Step 2b designs are decided fresh (0088)?

## Facts
Verified 2026-10-07.
- A Bun 1.4.2 binary compiled on Windows (`bun build --compile`, temp dir) ran SQLite 3.53.2 through both `bun:sqlite` and `node:sqlite` (`DatabaseSync`), each with WAL, FTS5 and `STRICT` tables; `bun:sqlite` also enforced foreign keys and stored `PRAGMA user_version`. 0078 had `node:sqlite` unverified.
- Pi 1.0.1 already reads `JARVIS_CODING_AGENT_DIR` for the config dir and `JARVIS_CODING_AGENT_SESSION_DIR` for sessions (`dist/config.js:438-439`, with `piConfig.name` "jarvis").
- `@earendil-works/pi-durable` is 1.0.4 on npm (modified 2026-10-05; the plan says 1.0.0), still "Experimental"; its SQLite storage is one database owned by one process, with no cross-process locking (`packages/durable/README.md:3,524-527` in the Pi v1.0.1 clone).
- Lesson L66 (durable storage) maps to 1.1: `busy_timeout` before WAL, `synchronous = NORMAL`, and a rebuildable index beside source-of-truth files.
- The plan's list had six items; item 5 bundled v4 Step 2b's connection and schema rules.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of eight: data dir; where client-naming config lives; overrides; driver; connection settings; schema rules; an unopenable, newer or locked DB; DB files. It also confirms section 7's Storage and Locations conventions | Every lasting storage choice is asked; failure behavior is decided before the build | Eight questions instead of six |
| The plan's six | Fewer questions | Connection and schema rules ride as one bundle; failure behavior is left to the build |

## Recommendation
The fresh agenda of eight.

## Choice
The fresh agenda of eight.

## Reason
Not given.

## Consequences
- Plan 1.1's list becomes the eight items, citing this record.
