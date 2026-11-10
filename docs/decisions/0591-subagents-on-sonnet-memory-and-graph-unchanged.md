# 0591 Subagents on Sonnet 5.5; memory notes and the graph unchanged

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V8, V13
- Date: 2026-11-10

## Question
0590 left three roles for research. Which model runs subagents (`task`), which writes the memory notes (`memory`), and which extracts the memory graph's facts?

## Facts
Verified 2026-11-10. The evidence and his usage (aggregates only) are in `D:/jocasta-spikes/models/evidence.md` and `D:/jocasta-spikes/models/usage.md`, outside the repo.
- **His spend**, 2026-10-27 to 2026-11-10 (omp's recorded costs, 12 days with use): $2,755.27 in all.
  - Subagents (`task`, Opus 5.5 at medium) cost $1,381.75, about $115 a day with use.
  - A median subagent run is 76.5 requests and $4.51.
  - Sonnet 5.5's prices are exactly half of Opus 5.5's.
- **Subagents:**
  - Anthropic calls Sonnet 5.5 "strongest at well-scoped everyday tasks" (https://www.anthropic.com/claude-sonnet-5-5) and advises `medium` effort for well-specified agentic work (https://platform.claude.com/docs/en/models/sonnet-5-5/whats-new-sonnet-5-5).
  - Terminal-Bench 4.0: Sonnet 70.6%, Opus 66.4%. Artificial Analysis measured 64% and 60%.
  - CursorBench 4.0: Sonnet 55.5%, against Opus's best of 57.8%; Opus at medium 52.5%.
  - FrontierCode 1.1: Opus at medium 54.6%, against Sonnet's best of 52.1% (https://www.anthropic.com/claude-opus-5-5).
  - At high effort and above, Sonnet's extra tokens cancel its discount (https://artificialanalysis.ai/articles/claude-sonnet-5-5).
  - No one has published both models at medium on the same benchmark.
- **Memory notes:**
  - mnemopi's notes in Jocasta's panes and the boss run on his `memory` role, despite the overlays' `llmMode: smol` (omp `mnemopi/backend.ts:559`). His own omp sessions (`memory.backend: local`) use `default` and `smol` instead.
  - About 43 calls in the window, at most $0.09 in all on Sonnet.
  - Haiku 5.5's system card finds Opus and Sonnet 5.5 "strictly better" on input hallucination, important omissions and false completion claims (https://www.anthropic.com/document/claude-haiku-5-5-system-card, section 6.2.3).
  - The one public test of filing a finished session's lasting notes (https://agent.nexus/blog/haiku-5-5-vs-sonnet-5-5): Sonnet at medium 71.9%, at low 70.9%; Haiku at high 69.1%, at low or medium 48–53%.
- **The graph:**
  - Graphiti's calls go through `omp auth-gateway stdio`, which keeps a role selector's model and drops its effort (`cli/auth-gateway-stdio.ts:39-41`). The main model on `@default` therefore runs Opus 5.5 at low effort, and Haiku runs with thinking off.
  - The gateway drops `response_format`, so only the prompt asks for JSON (`src/frontier/server/llm-endpoint.ts:121-127`).
  - Graphiti has never run on his PC; Docker wasn't running. At about 1.3 episodes a day, the main model would cost about $0.83 a day on Opus, $0.41 on Sonnet and $0.02 on Haiku (estimates).
  - Following `@memory` would need code: the endpoint sends only `@default` or `@smol`, and `jocasta.yaml`'s `graph.main` takes `provider/id`.

## Options
Subagents (`task`):

| Option | Pros | Cons |
|---|---|---|
| Sonnet 5.5 at medium | About $57 a day less, on half his spend; Anthropic's pick for scoped work; level with or ahead of Opus on the agent and terminal benchmarks | Opus leads on changes that must merge cleanly, so a subagent's edit may need more fixing by its thread |
| Keep Opus 5.5 at medium | The strongest edits; no change | About $57 a day more |
| Sonnet 5.5 at high | Closer to Opus on harder subtasks | Its extra tokens cancel most of the saving |

Memory notes (`memory`):

| Option | Pros | Cons |
|---|---|---|
| Keep Sonnet 5.5 at medium | The best measured quality for the job; pennies at his volume | None at this volume |
| Sonnet 5.5 at low | Within a point of medium in the one test | Saves fractions of a cent; a 10-job test |
| Haiku 5.5 at high | The cheapest | Close only at high effort, and its scores swing; misstates sources more |

The graph:

| Option | Pros | Cons |
|---|---|---|
| Keep following `default` and `smol` | Opus extracts most faithfully; under $1 a day at the gateway's low effort; no code | The most expensive per episode if the graph gets busy |
| Follow the `memory` role (Sonnet 5.5) | Half Opus's price; Graphiti's pattern of a mid-size main model | Code in the endpoint and `jocasta.yaml`'s checks to save about $0.40 a day; Sonnet runs with thinking on |
| Haiku 5.5 for the main model too | Graphiti's own Anthropic default; cents | Thinking off and only the prompt enforcing JSON; misstates sources more |

## Recommendation
Sonnet 5.5 at medium for subagents; the memory notes and the graph unchanged.

## Choice
- `task`: Sonnet 5.5 at medium (`anthropic/claude-sonnet-5-5:medium`). It falls back to Opus 5.5 at medium, then Sonnet 5 at high.
- `memory`: Sonnet 5.5 at medium, unchanged.
- The graph keeps following `default` and `smol` (0463 unchanged).

## Reason
He picked the recommended option in each question.

## Consequences
- These change only his omp config, outside the repo. With 0590, every role has its model.
- The graph's cost is an estimate until it runs on his PC.
