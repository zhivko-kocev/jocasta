# 0334 Graphiti in v1, through Jarvis

- Status: decided
- Kind: product
- Phase/slice: 5/5.0, 5.9
- Date: 2026-10-08

## Question
Does Graphiti join v1 now, and how does it reach a model?

## Facts
Verified 2026-10-08.
- 0278 kept Graphiti only if a 30-question spike showed at least 5 more correct answers than `memory.md` plus `history_search`. Since then billion-context's `search_context` replaced `history_search` (0323), a stronger baseline.
- graphiti-core is 0.30.2 (PyPI, 2026-09-08; the plan said 0.29.2), Python 3.10–3.x. Backends: Neo4j 5.26, FalkorDB 1.1.2, Neptune; Kuzu is deprecated. Model clients: OpenAI by default, Anthropic, Gemini, Groq, and any OpenAI-compatible endpoint (README). Anthropic has no embeddings model; local embeddings come from the `sentence-transformers` extra (PyTorch). Telemetry is on unless `GRAPHITI_TELEMETRY_ENABLED=false`.
- Docker Desktop 29.5.2 is installed and running (0071).
- Graphiti can't sign in with his Claude subscription. Jarvis can serve a local OpenAI-compatible endpoint that sends Graphiti's calls through Pi, so they bill like the rest of Jarvis (0336).
- He re-sent his memory design (0278's Reason), in which Graphiti holds the long-term knowledge and relations.

## Options
| Option | Pros | Cons |
|---|---|---|
| Graphiti in v1 as his design says, no spike first | His full design | A Python sidecar and model calls per stored item, unmeasured before the build |
| Keep the spike (0278) | Measured before it stays | An API key and installs for a test; his time to grade 30 answers |
| Graphiti after v1 | Leanest | No relation queries in v1 |

Route: through Jarvis's endpoint | an Anthropic API key | an OpenAI API key.

## Recommendation
Keep the spike; route through Jarvis.

## Choice
Graphiti in v1 with no spike first, routed through Jarvis:
- FalkorDB (`falkordb/falkordb`) and a Graphiti service built from a Dockerfile in Jarvis on `python:3.12-slim` with `graphiti-core[falkordb,sentence-transformers]` 0.30.2, both in containers (0070); telemetry off.
- Graphiti's model calls go to a local OpenAI-compatible endpoint served by Frontier's server, which sends them through Pi with his login; embeddings run locally in the Graphiti container.
- The 30-question comparison becomes an eval he can run any time after the build, with its cases in his config dir (0035).
- Installs approved: the two images and the Python packages inside them.

## Reason
"build it we test in prod :D, is there a way we can see after we build everyyting or?"; on the route: "Through Jarvis"; installs: "Approve".

## Consequences
- Amends 0278: Graphiti no longer waits for a spike; 5.0 becomes the after-build comparison and 5.9 is built in phase 5.
- 5.4's promotion sends "useful" items to Graphiti and important rules to `memory.md` proposals; 5.5's reads add the graph's search.
- Differs from the recommendation, which kept the spike.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
- Amended by 0430: Graphiti takes a per-run token and checks Host.
