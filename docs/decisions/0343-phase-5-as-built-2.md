# 0343 Phase 5 as-built, subscription shaping and graph plumbing

- Status: decided
- Kind: product
- Phase/slice: 5/5.9
- Date: 2026-10-08

## Question
Are the remaining phase 5 choices accepted, and when does the account lookup run?

## Facts
- omp looks up the Anthropic account id at login and token refresh (`registry/oauth/anthropic.ts`); the build did it once per token per process, before the first subscription request.
- The Graphiti service, FalkorDB and the `/llm/v1` endpoint bind to 127.0.0.1 only, which no other computer on the network can reach; other programs on this computer can. FalkorDB publishes no port.

## Options
| Option | Pros | Cons |
|---|---|---|
| Accept; account lookup at login and refresh, as omp | Matches omp; no extra call per run | Needs a hook on Pi's login and refresh |
| Accept as built | Done | One extra call per run |

## Recommendation
Accept; lookup at login and refresh.

## Choice
- `JARVIS_CLAUDE_CODE_VERSION` pins the Claude Code version Jarvis claims; normally unset. Now (0377): dropped with Jarvis's own shaping.
- `~/.jarvis/agent/install-id` holds a random id made once, hashed into the device id. Now (0377): dropped.
- The account id is looked up at login and token refresh, as omp does, and kept with the login. Now (0377): dropped.
- Claude Code shaping runs after other extensions' request edits. Now (0377): `@gotgenes/pi-anthropic-auth` loads first and shapes inside its provider.
- The Graphiti service answers `/episodes`, `/search` and `/health` on 127.0.0.1 with no password.
- Jarvis serves Graphiti's model calls at `/llm/v1` on Frontier's port or a lone harness's own port, token-protected; the container reaches it through `host.docker.internal`.
- Jarvis writes the Graphiti build files into `~/.jarvis/agent/containers/graphiti/` and overwrites them when it updates.

## Reason
"we can do account lookup as omp on login maybe, services on localhost cannot be accesed from the same network from a different pc right?"

## Consequences
- Appendix I: `JARVIS_CLAUDE_CODE_VERSION`, `~/.jarvis/agent/install-id`, `~/.jarvis/agent/containers/`.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
- Amended by 0390: the companion serves `/llm/v1` through omp's model registry and logins.
