# 0538 K1: the cost ledger is a projection of the session files, on omp's cost, shown through thread_status

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-08

## Question
The harness-OS layer (0532) measures before it changes anything (cost per successful task, cache traffic, waste). Where does the ledger live, which cost does it use, where do its waste signals show, and how are its parsers checked?

## Facts
Verified 2026-11-08.
- Today's spend is a projection: `src/frontier/server/sessions.ts` sums each assistant message's and `model_usage` entry's `usage.cost.total`, each entry once by id (0452, 0464); `catalog.db` (`src/shared/schema.ts`) has no table of model requests.
- omp writes `usage {input, output, cacheRead, cacheWrite, cost{…, total}}` per message, its API-equivalent cost, as `omp stats` counts it.
- Under 0537 the product reads omp only; the benchmark also reads Claude Code transcripts (rewritten in place on resume and compaction, per tokscale's docs) and opencode's `opencode.db`, and keeps every trial's transcript.
- Studies price savings at the bill, with cache read and cache write apart (arXiv 2607.12161; JetBrains' rtk benchmark).
- tokscale (github.com/junhoyeo/tokscale, MIT, v4.21.0 of 2026-11-02, Windows binary) reads omp, Claude Code and opencode; token-optimizer's waste catalogue is PolyForm Noncommercial (ideas only).

## Options
| Question | Options (pros / cons) |
|---|---|
| Store | Projection of the session files and relay events (one truth, no migration / history re-reads files) · A usage table in `catalog.db` (fast history / a second copy, a migration) |
| Cost | omp's cost in the product, one pinned price table for the benchmark (HUD matches `omp stats`; fair across harnesses) · One Jocasta table everywhere (HUD stops matching `omp stats`) |
| Shown | `thread_status` and the benchmark (no new UI) · Also a sidebar token (crowds the row) · Benchmark only (the boss can't tell him) |
| Parser check | tokscale, bench-only (catches format quirks / an install) · Own fixtures only (no install / misses quirks) |

## Recommendation
Projection; omp's cost with a price table for the benchmark; `thread_status` and the benchmark; tokscale, bench-only.

## Choice
- **Store:** the ledger is a projection, computed by the service from omp's session files (and the full guest's live events, if 0536's spike leads to building it), with no new table; the benchmark computes the same rows from its saved transcripts and opencode's export.
- **Rows:** per model request: thread, session, agent, time, model, uncached input, cache read, cache write, output, cost; per tool call: tool, an argument hash, file path, result size, error.
- **Cost:** the product uses omp's `usage.cost`; the benchmark prices every arm's tokens with one pinned price table (LiteLLM's model prices, committed with the benchmark).
- **Waste signals:** re-reads of an unchanged file, the same call repeated, an oversized result, repeated failures of one tool, a cache miss (a large cache write after more than 5 minutes idle), compactions.
- **Shown:** each thread in the boss's `thread_status` reply gains one line:
  `spend $1.84 · cache read 71% · 3 re-reads of unchanged files · 1 repeated call · 1 cache miss (7 min idle) · 2 compactions`;
  the benchmark rows carry the same counts; K9 and K10 read them. No new UI.
- **Parser check:** tokscale is installed for the benchmark only (pinned Windows binary, not shipped); a parity test compares the ledger's totals per session with `tokscale models --json`.

## Reason
"Projection, no new store"; "omp's cost; a price table for the bench"; "thread_status and the bench"; "Yes, bench-only".

## Consequences
- Install approved: tokscale, bench-only (0385's list gains it).
- Claude Code's in-place rewrites don't matter: the benchmark reads each trial's transcript once, at its end.
- Amended by 0546: the line gains peak context (percent and tokens of the model's window).
