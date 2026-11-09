# 0463 Graphiti's model calls through omp's auth gateway

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
The companion's model endpoint for Graphiti imports omp internals. How does it reach omp's models?

## Facts
Verified 2026-10-23.
- `llm-endpoint.ts` imports omp internals (`resolveRoleSelection` from `src/config/model-resolver.ts`, not exported; `pi-ai` `completeSimple`, lines 252-262, 311); that seam is re-checked on every omp bump (plan.md:675).
- `omp auth-gateway stdio` exists (`cli/auth-gateway-stdio.ts`) and serves chat-completions on his logins and roles with fallbacks: `model` takes `@smol` or comma lists and walks `retry.fallbackChains` (omp `auth-broker-gateway.md` "stdio").
- The gateway's attribution headers (`x-omp-install-id`, hostname, app) are not forwarded upstream (omp `auth-broker-gateway.md`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Spawn `omp auth-gateway stdio` | A public omp command instead of internals; fallback chains for free | One child process |
| Keep importing omp's internals | No change | Breaks when omp moves an unexported function |
| `omp -p` only | Public | A process per call; no fallback chains |

## Recommendation
Spawn `omp auth-gateway stdio`.

## Choice
The companion's model endpoint spawns `omp auth-gateway stdio` instead of importing omp's internals.

## Reason
"Spawn `omp auth-gateway stdio`".

## Consequences
- Amends 0390 (how `llm-endpoint.ts` reaches omp's models).
- The internal import leaves plan 5.4/5.5's bump re-check list; the gateway command takes its place.
