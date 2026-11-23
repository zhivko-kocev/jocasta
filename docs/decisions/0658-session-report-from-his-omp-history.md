# 0658 A report on his omp sessions: where the tokens went, what could be deterministic

- Status: v1
- Kind: process
- Phase/slice: fork build, review
- Date: 2026-11-23

## Question
In the review he asked for "a token doctor or whatever it is called, something to analyze the sessions and see what can be optimized or made more optimal or deterministic from my past usage of omp". What, and how?

## Facts
Verified 2026-11-23:
- tokendoctor (`github.com/localixai/tokendoctor`, MIT) replays Claude Code's transcripts at smaller compaction windows with no model call and changes Claude Code's settings. It reads `~/.claude` only, where he has 7 projects; his work is in omp.
- `~/.omp/agent/sessions` holds 3,516 session files (5.5 GB), 739 since 2026-10-24.
- Jocasta already prices omp sessions (`src/shared/ledger.rs`, 0538) and indexes them for search (`history.rs`, 0603).

## Options
- **A one-off report first, then decide.** Pros: no model calls, no product change; findings become proposals he picks from. Cons: a report, not a tool he reruns.
- **A Jocasta command now, tokendoctor-style** (replay, propose, apply, undo). Pros: reusable. Cons: a new feature with its own grilling before evidence says what it should do.
- **After v1.** Pros: nothing now. Cons: nothing learned now.

## Recommendation
A one-off report first.

## Choice
His answer of 2026-11-23: the report first. A throwaway script, with no model calls, reads every omp session on his PC and reports: tokens and cost by model, role, project and subagent (priced as the ledger does); context per request and the cache-read share; compactions and their cost; a replay at smaller compaction thresholds as tokendoctor does it; identical tool calls and command sequences repeated across sessions (candidates for a script, skill or mise task); repeated failures and retries; files read again and again. The report and its data stay outside the repo (they name his projects), in `D:/thesis/data/doctor/`, with a summary for him; its findings come to him as proposals.

## Reason
His words above, and the report-first answer; 0599's rule that what can be deterministic is.

## Consequences
- Nothing in the product changes from this record; each proposal he takes gets its own record.
- A command for it, if he wants one later, is a feature request (0018).
