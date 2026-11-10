# 0592 Compaction: handoff first, shake and the image method last

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V9, V13
- Date: 2026-11-10

## Question
0399 put shake first and 0448 kept that order at 300,000 tokens. His threads now run on Opus 5.5, Fable 5.2 and Sonnet 5.5. Which method runs first? Does idle compaction stay? Do the other methods get a place in Jocasta's workflows?

## Facts
Verified 2026-11-10 against omp 18.18.6's npm packages. The reports are in `D:/jocasta-spikes/settings/compaction.md` (section 8 for shake), outside the repo; his sessions as aggregates only.
- **The methods:**
  - **shake:** no model call. It turns big old tool outputs into stubs, recoverable through `artifact://`, and keeps the newest 16k tokens.
  - **handoff:** the live model writes a structured note (goal, constraints, progress, decisions, context, next steps), and about 20k recent tokens stay word for word. It can't run on an overflow (`session-maintenance.ts:214-215`).
  - **soft:** a plain model summary.
  - **snapcompact:** no model call. It turns the old conversation into images, so it needs a model that reads images.
  - **remote:** Anthropic's compaction on its servers (beta `compact-2026-09-04`): a readable summary plus a signed block. The watcher skips it, so Graphiti never gets it (`watcher.ts:15-17`).
- **Shake on his models:**
  - Opus 5.5, Fable 5.2 and Sonnet 5.5 carry the catalog's `thinking-prefix-binding` (`pi-catalog` `classes/anthropic.kdl`).
  - The prefix guard (`session-maintenance.ts:786`, `:836`) limits only `dropUseless` and `supersedeReads`. A threshold or idle shake rewrites inside the warm cached prefix (`shake.ts:41`).
  - A shake also drops the thinking of every earlier assistant turn from the request (`shake.ts:443`; `transform-messages.ts:922-941`).
  - It falls through to the next method only while the context stays above 80% of the trigger, and never on idle (`session-maintenance.ts:6541`).
- **Cost at 300k on Opus 5.5** (estimates from the source and Anthropic's prices):

  | Event | Cost |
  |---|---|
  | A turn with the cache warm | $0.06 |
  | A shake (a new cache write; lands at 240k at most) | about $1.20 |
  | A handoff, cache warm | about $0.46 |
  | Remote, cache warm | about $0.47 |
  | Handoff or remote, cache cold | about $1.90 |

- **Waiting:**
  - With shake first, omp prepares no summary in the background (`compaction-methods.ts:131-134`; the grace band is off, `session-maintenance.ts:2386-2394`). A handoff after a shake runs while he waits: likely tens of seconds to a couple of minutes at xhigh [INFERENCE].
  - With handoff or remote first, omp builds the summary from about 268k and swaps it in at 300k.
- **Idle compaction** fires about 53 min 55 s after the last request, before the 1-hour cache's refresh point at 54 minutes (`idle-maintenance.ts:268-277`; `cache-warmer.ts:74-77`). A summary there reads the warm prefix cheaply. After a lapse, re-caching 300k costs about $1.50. omp's default is idle off at 200,000.
- **omp's default order** is `[remote, snapcompact, handoff, shake, soft]`.
- **His last 30 days**, plain sessions on that order: 45 remote, 14 snapcompact and 4 handoff (subagents) compactions, no soft. Main sessions compacted at a median of 601k tokens; 23 of 130 passed 300k.

## Options
Which method first:

| Option | Pros | Cons |
|---|---|---|
| Handoff first: `[handoff, remote, soft, shake, snapcompact]` | One structured note everywhere, the same as the task-end `/handoff`; built in the background; feeds Graphiti; remote takes overflows; shake and snapcompact stay as no-model last resorts | A model call each time, about $0.46 at 300k |
| Remote first: `[remote, handoff, soft, shake, snapcompact]` | Anthropic's own method, made for these models; built in the background | Graphiti gets nothing from it unless the watcher learns to read it (code) |
| omp's default: `[remote, snapcompact, handoff, shake, soft]` | omp's tested default | snapcompact (lossy images) second |
| Keep shake first (0399) | No change; shake loses nothing | About 2.5 times the cost; blocks his turn; drops earlier thinking |

Idle compaction:

| Option | Pros | Cons |
|---|---|---|
| On at 100,000 (0399) | A session left for an hour comes back small and cheap; the note feeds Graphiti | A session only paused comes back summarised beyond its last ~20k tokens |
| On at 200,000 | Only big sessions get summarised | Smaller ones re-cache in full |
| Off (omp's default) | Nothing is summarised while he's away | Coming back after an hour re-caches the full context |

The other methods in the workflows:

| Option | Pros | Cons |
|---|---|---|
| The order and manual commands | No code; each method runs where it fits; any is one command away | Shake never runs at a moment it would be free |
| Shake on a cold reopen | Jocasta types `/shake` into a pane reopened more than an hour after its last request, when a rewrite costs nothing extra | New code; small gain, since idle compaction already summarises sessions above 100k before the cache lapses |

## Recommendation
Handoff first; idle compaction on at 100,000; the order and manual commands.

## Choice
- `compaction.methodOrder: [handoff, remote, soft, shake, snapcompact]` everywhere: his `~/.omp/agent/config.yml`, `config/omp-config.yml`, the per-project overlay, `config/boss.yml`, and A1's arm (`evals/variants/a1.yaml`), which carries the overlay's settings.
- `compaction.thresholdTokens: 300000`, asked again and kept (0448). His config's `thresholdPercent: 60` goes.
- `idleEnabled: true` and `idleThresholdTokens: 100000`, unchanged.
- No new workflow. The other methods run as fallbacks in the order:
  - remote takes an overflow, or a failed handoff;
  - soft takes a failed remote;
  - shake keeps a session going when no model call works;
  - snapcompact is the last resort.
- By hand, he can still run `/compact remote|soft|snapcompact` (`session/compact-modes.ts:40-55`), `/shake` (more aggressive than the automatic one: it protects only the last 4,000 tokens against 16,000, `pi-agent-core` `compaction/shake.ts:47-66`) or `/handoff <focus>`.

## Reason
"okay handoff first but how will the ordering be and how we can use the other stuff in the workflows". After the order was explained he picked "Order and manual commands", and for idle "On at 100,000". For the threshold he picked 300,000 tokens.

## Consequences
- Amends 0399 (handoff first, shake fourth) and 0448 (the overlay's and the boss's order). 0399's own recommendation had been handoff first.
- Every automatic compaction now writes a handoff, which the watcher feeds to Graphiti (0400).
- A1's arm follows the overlay. No benchmark row has run on this build (0533), so nothing measured changes.
- Plan appendix I lists the new order.
