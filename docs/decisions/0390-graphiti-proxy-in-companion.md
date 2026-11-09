# 0390 The Graphiti proxy in the companion

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Who pays for Graphiti's model calls?

## Facts
Verified 2026-10-13.
- Each `/episodes` request carries `llm: {base_url, api_key}`, today pointing at Jarvis's `/llm/v1` over Pi's model registry (0341, 0343).

## Options
| Option | Pros | Cons |
|---|---|---|
| Companion keeps the proxy | On his subscription through omp's logins | Graph writes wait while the companion is closed |
| API key for Graphiti | Always available | Billed per token |
| No Graphiti | Nothing to run | omp's SQLite memory only |

## Recommendation
Companion keeps the proxy.

## Choice
The companion serves `/llm/v1` for Graphiti through omp's model registry and his omp logins; graph writes wait while the companion is closed.

## Reason
"Companion keeps the proxy".

## Consequences
- `src/frontier/server/llm-endpoint.ts` moves from Pi's registry to omp's; the main and small models are omp roles set in `jarvis.yaml` (`graph.main`, `graph.small`).
- Amended by 0463: the model endpoint spawns `omp auth-gateway stdio` instead of importing omp's internals.
