# 0281 Phase 5 remaining slices

- Status: decided
- Kind: product
- Phase/slice: 5/5.3–5.9
- Date: 2026-10-08

## Question
Which remaining phase 5 slices are in v1?

## Facts
- With 0277–0280: confirmed proposals replace old lines, so contradictions and quarantine need nothing extra; per-project files can't cross projects.
- The recommendation put 5.6, 5.7 and 5.8 after v1; he ticked all three into v1.

## Options
| Option | Pros | Cons |
|---|---|---|
| 5.8 importer in v1 | Memory knows his projects from day one | Mining runs cost tokens |
| 5.7 nightly consolidation in v1 | Tidy memory | A nightly model call |
| 5.6 leak check in v1 | A test proves sealing | A test to write |

## Recommendation
5.6, 5.7 and 5.8 after v1; 5.4's contradictions and 5.3's quarantine need nothing extra.

## Choice
- v1: 5.6 leak check (a test that no project's memory, decision log or graph data appears in another project's context); 5.7 nightly consolidation (proposals to merge duplicates and drop stale lines in each `memory.md`, confirmed by him); 5.8 importer (a one-off mining of his omp and Claude Code transcripts into `memory.md` proposals he confirms).
- Nothing extra: 5.4 contradictions (an accepted proposal replaces the old line) and 5.3 quarantine (every entry is confirmed).
- 5.9 Graphiti is decided by 0278's spike.

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- The nightly run happens inside Frontier's server (0253) when it is running.
- Mining transcripts reads ~/.claude and omp files once, as an import he starts; it doesn't reopen 0157 or 0231.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
