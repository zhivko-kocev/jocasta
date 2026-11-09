# 0336 Plan limits through pi-anthropic-auth

- Status: decided
- Kind: product
- Phase/slice: 5/5.9 (cross-cutting: 0062)
- Date: 2026-10-08

## Question
Does Jarvis bill his Claude subscription's plan limits, as omp does, instead of per-token extra usage?

## Facts
Verified 2026-10-08.
- Pi 1.1.0 warns: "Third-party harness usage draws from extra usage and is billed per token, not your Claude plan limits" (0062).
- omp presents itself to Anthropic as Claude Code: `packages/ai/src/providers/anthropic-identity.ts` ("Claude Code's legacy cloaking" metadata ids, Claude Code's wire-only tool prefix), plus Claude Code's beta headers and TLS profile (omp @89d2610, `D:/learn-harnesses/repos/oh-my-pi`).
- `@gotgenes/pi-anthropic-auth` 3.4.2 (MIT, 2026-10-01, 13.2K downloads a month, needs Pi 0.86 or later; https://pi.dev/packages/@gotgenes/pi-anthropic-auth) shapes Anthropic OAuth requests with Claude Code's billing header, so they count against plan limits. It covers interactive turns, compaction and extension calls through `ctx.modelRegistry.streamSimple()`, not calls through pi-ai's `compat.streamSimple`. It follows Claude Code's version floor and retries once when Anthropic raises it.
- Claude Code's legal docs (0062): OAuth is "intended exclusively for" Claude Code and Anthropic's native apps; "Anthropic reserves the right to take measures to enforce these restrictions".

## Options
| Option | Pros | Cons |
|---|---|---|
| Add the extension | Plan limits instead of per-token extra usage | Risks his account under Anthropic's terms; breaks when Anthropic changes its checks |
| Don't add it | No enforcement risk | Per-token extra usage |
| Decide after v1 | — | Per-token until then |

## Recommendation
Don't add it.

## Choice
Now (0377): the package is back: 3.4.2, pinned exactly, compiled in through a typed boundary in place of Jarvis's own shaping.

Earlier (0337): Jarvis's own shaping, modeled on omp; the package is removed.

Originally: add `@gotgenes/pi-anthropic-auth` 3.4.2, pinned exactly, loaded with Jarvis's extensions; bumped deliberately.

## Reason
"Add it (my account, my risk)"; earlier: "what is omp then using how am i working with my subscriptions without extra usage".

## Consequences
- Every Jarvis call to Anthropic through Pi's registry, Graphiti's included (0334), counts against plan limits.
- Jarvis's own model calls must go through `ctx.modelRegistry.streamSimple()` or Pi's session to be shaped.
- A Pi bump re-checks the extension's peer range; an Anthropic change can stop it until a new version ships.
- Differs from the recommendation.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
