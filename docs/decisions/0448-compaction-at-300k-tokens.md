# 0448 Compaction in the pane settings, at 300,000 tokens

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
0399's 60% threshold is a percent of the model's window, and his live config doesn't carry 0399's compaction block. When does a pane compact, and where is that set?

## Facts
Verified 2026-10-23.
- omp takes `thresholdPercent` of the model's window (`pi-agent-core/src/compaction/compaction.ts:412-417`); his `default`, `task` and `plan` roles are `claude-opus-5-5` with a 1,000,000-token window (`~/.omp/agent/models.db`), so 60% = 600,000 tokens.
- A positive `compaction.thresholdTokens` takes precedence over the percent (`session/model-compaction-threshold.ts`; `compaction.ts:389-401`); on a window of 300k or less it falls back to omp's reserve budget, `window − max(15%, 16384)` (`compaction.ts:399`).
- Thariq (Anthropic, 1M model): "some level of context rot happens around ~300-400k tokens" (`tips/claude-thariq-tips-16-apr-26.md:28`, https://github.com/shanraisshan/claude-code-best-practice).
- Dex Horthy's 40% is for beginners on 200k windows (`videos/claude-dex-mlops-community-24-mar-26.md:95`, `:185`, `:187`).
- His live config has only `compaction.thresholdPercent: 60` (`~/.omp/agent/config.yml:27-28`), so omp's default method order (`remote` first, omp `docs/compaction.md:514`) applies and idle compaction is off; `remote` summaries never reach Graphiti (`src/frontier/server/watcher.ts:15-17`).
- omp supports `/handoff [focus]` (`omp-src/slash-commands/builtin-lifecycle.ts:211`); the companion sends a bare `/handoff` (`service.ts:1310`).

## Options
| Option | Pros | Cons |
|---|---|---|
| 300,000 tokens, set in the overlay and boss config | Below the cited 300–400k rot range; works whatever his global config says | Small windows fall back to omp's reserve budget |
| 40% | Common advice | Measured on 200k windows; 400k on his 1M models |
| Keep 60% | No change | 600,000 tokens, past the cited range |
| `compaction.modelThresholds` per model | Keeps 60% for small windows | One entry per model family |

## Recommendation
300,000 tokens, set in the overlay and boss config.

## Choice
The per-project overlay and the boss config set 0399's compaction block themselves: `compaction.methodOrder: [shake, handoff, soft, snapcompact, remote]`, `compaction.thresholdTokens: 300000` (instead of `thresholdPercent: 60`), `compaction.idleEnabled: true`, `compaction.idleThresholdTokens: 100000`. The task-end `/handoff` Jarvis sends carries a fixed focus line (text chosen by the builder: what the next task needs — the approved plan's state, open questions, decisions).

## Reason
"300,000 tokens, set in the overlay and boss config".

## Consequences
- Amends 0399 (`thresholdPercent: 60` becomes `thresholdTokens: 300000`; the block moves into the pane settings).
- The focus line is a shipped prompt chosen by the builder and listed as built (0363).
- Plan appendix I lists the overlay's compaction keys.
- Amended by 0592: the overlay's and boss's order is `[handoff, remote, soft, shake, snapcompact]`; 300,000 tokens and idle at 100,000 stay.
