# 0078 Research list: FTS5 in a compiled binary, failing `tool_result` handlers

- Status: decided
- Kind: process
- Phase/slice: 0/5.4
- Date: 2026-10-07

## Question
Does plan 5.4's list of unverified facts take the two answers found in the harness comparison?

## Facts
- `bun:sqlite` FTS5 works inside a `bun build --compile` binary on Windows. Run 2026-10-07 with Bun 1.4.2: a compiled probe created an `fts5` table, matched a row and returned a `bm25()` score. `node:sqlite` was not tested.
- Pi 1.0.1 skips a `tool_result` handler that throws: `emitToolResult` catches the error, reports it through `emitError` and moves on (`packages/coding-agent/src/core/extensions/runner.ts:1183-1225`). A `tool_call` handler failure blocks the tool (plan 5.4), so the two hooks fail in opposite directions. A redaction handler that throws (2.6) leaves the result unredacted.

## Options
| Option | Pros | Cons |
|---|---|---|
| Both: mark the `bun:sqlite` half answered and add a phase 2 item on failing `tool_result` handlers | The list shows what is known; 2.6 opens with the risk named | Phase 1's fact check still re-runs the probe at the pinned Bun |
| Only the phase 2 item | Leaves FTS5 to the phase 1 fact check | The list keeps an answered question |
| Neither | No change before the phase openings | 2.6 may not see the fail-open behavior in time |

## Recommendation
Both.

## Choice
Both.

## Reason
Not given.

## Consequences
- Plan 5.4: the phase 1 item keeps only `node:sqlite`; phase 2 gains the question of how redaction fails closed when a `tool_result` handler throws.
- Plan appendix B's SQLite row now says FTS5 works in a compiled binary with `bun:sqlite` and is unverified for `node:sqlite`.
- Phase 1's fact check re-runs the probe at the Bun version pinned then (0042).
