# 0489 Graphiti's network limit comes after v1

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
0484 (part 3) asked for Graphiti's container to reach only the host's model gateway. The build stopped under 0020: on Docker Desktop that needs a forwarding container. Build it in v1, or is what was built enough for now?

## Facts
Verified 2026-10-24.
- Built for 0484: the Graphiti image runs as user `graphiti` (UID 10001), downloads nothing at run time (telemetry off, Hugging Face offline, the model baked in), and calls only the companion's `/llm/v1`; its port is published on 127.0.0.1 only and bearer-protected (0430).
- Docker's docs: an `internal: true` network has no route out of its bridge; on Docker Desktop `host.docker.internal` is the Windows host reached through the VM, so Graphiti on an internal network likely can't reach `/llm/v1`; unverified here because Docker wasn't running.
- The working design is a small forwarding container on both networks, passing only the published port and the callback, with `server.ts`'s Host allowlist or `callback.ts`'s URL changed.
- 0485 put OS isolation for thread panes after v1.

## Options
| Option | Pros | Cons |
|---|---|---|
| Non-root and no downloads for v1; the network limit after v1 | No extra container now; done with the other isolation work | A compromised dependency inside Graphiti could still reach the internet |
| The forwarding container in v1 | Graphiti can't reach the internet even if a dependency were malicious | One more container and two code changes; delays v1 |

## Recommendation
Non-root and no downloads for v1; the network limit after v1.

## Choice
For v1, Graphiti runs as a non-root user and downloads nothing at run time; limiting its network to the host's model gateway (the forwarding container) comes after v1, with the OS isolation of thread panes (0485).

## Reason
"Enough for v1; network limit after v1".

## Consequences
- Amends 0484 (part 3's network item).
- `docs/status.md` lists it among the after-v1 items.
