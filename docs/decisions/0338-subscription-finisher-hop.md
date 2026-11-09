# 0338 A finisher hop after billion-context for subscription requests

- Status: decided
- Kind: product
- Phase/slice: 5/5.9 (cross-cutting: 0323, 0337)
- Date: 2026-10-08

## Question
billion-context's proxy rewrites Jarvis's shaped subscription requests. How do 0337's shaping and 0323's compression live together?

## Facts
Found in the build, offline, with a fake Anthropic endpoint and a fake `sk-ant-oat01-…` token (`test/anthropic-auth/proxy.test.ts`):
- Through the proxy, every Claude Code header, the billing block, the identity block, Jarvis's prompt, the tool prefix and the metadata arrive intact.
- The proxy adds `x-bili-hop`, `x-bili-plugin`, `x-bili-plugin-context-window`, `x-bili-plugin-conversation`, `x-bili-plugin-max-output`, `x-bili-plugin-model`, `x-bili-tunnel` and `x-session-id`; adds a fourth system block (~10.7 KB of its compression rules); and tags each message `<acp tokens=… type=…>`.
- The body changes after Jarvis attests it, so the `cch` checksum never matches what arrives, and the billing suffix is computed from a first user text that no longer matches.
- billion-context's hook rewrites each request to `<proxy>/bili/<original url>`, so the outbound TLS connection is the proxy's (`dist/agent/pi-native.js`, `installNativeFetchIntercept`).
- billion-context has a passthrough header, which skips its compression.

## Options
| Option | Pros | Cons |
|---|---|---|
| Finisher hop: Jarvis's request goes through the proxy to a local Jarvis endpoint that shapes last (drops `x-bili-*` and `x-session-id`, recomputes billing and `cch` on the final body) and sends to Anthropic with Jarvis's TLS settings | Compression and plan limits both work | One more local hop; more code |
| Bypass the proxy for subscription requests | Simple | No compression on his main model; reopens 0323 |
| Accept as built | Nothing to do | Subscription requests likely bill as extra usage |

## Recommendation
Finisher hop.

## Choice
Now (0377): dropped; billion-context's proxy stays on subscription requests with no finisher, and the package shapes before the proxy.

Originally: Finisher hop.

## Reason
Chosen from the options.

## Consequences
- 0337's shaping moves its last steps (headers, billing, attestation, TLS) to the finisher, which runs in every Jarvis process that sends model requests (Frontier's server, a lone harness, the importer).
- The proxy's own rules block and `<acp>` tags stay in the body; the billing seed follows omp's current rule (0339).
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
