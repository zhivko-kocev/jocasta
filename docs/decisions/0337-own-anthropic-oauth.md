# 0337 Jarvis's own Anthropic subscription shaping, modeled on omp

- Status: decided
- Kind: product
- Phase/slice: 5/5.9 (cross-cutting: 0062, 0336)
- Date: 2026-10-08

## Question
Does Jarvis keep `@gotgenes/pi-anthropic-auth` (0336), or match omp's subscription handling?

## Facts
Verified 2026-10-08 in omp @89d2610 (`D:/learn-harnesses/repos/oh-my-pi`, MIT).
- omp has its own Anthropic provider (`packages/ai/src/providers/anthropic.ts`, 6,040 lines). Its subscription handling is spread through it: Claude Code's identity and metadata ids (`anthropic-identity.ts`, 136 lines), Claude Code's beta headers (`anthropic-wire.ts`), a wire-only tool-name prefix, and Claude Code's TLS profile (`anthropic-client.ts`).
- The package adds Claude Code's billing block to the system prompt on top of Pi's transport, and drops sections of Pi's prompt, including a `<docs>` section 0171 keeps (AuthExt's reading of its source).
- AGENTS.md: files under `D:/learn-harnesses/` are read-only and never copied into the repo. The repo is public (0035).
- billion-context's proxy forwards Jarvis's model requests (0323, 0325), so anything at the TLS layer is the proxy's, not Jarvis's [INFERENCE, measured in the build].

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep the package | Least code; maintained upstream | Less than omp does; prompt sections dropped |
| Jarvis's own, modeled on omp, no copying | omp's full behavior | Ours to maintain when Anthropic changes its checks |
| Port omp's code with credit | Closest to omp | Breaks the reference-files rule |

## Recommendation
Keep the package.

## Choice
Now (0377): dropped; Anthropic refused it on 2026-10-09, and `@gotgenes/pi-anthropic-auth` shapes instead.

Originally: Jarvis writes its own, with omp's full logic, without copying omp's files; a way to keep it in step with omp is found and kept.

## Reason
"we should write our own but full omp logic. then we find a way to maintain it and keep it up to date because omp does better job than the package"

## Consequences
- Amends 0336: the package is removed; Jarvis's own extension does the shaping, for every Anthropic OAuth request through Pi's registry, Graphiti's included (0334).
- How it stays in step with omp is built with it and decided when its as-built items come back.
- Differs from the recommendation.
- Amended by 0389: memory is omp's memory backend plus Graphiti fed by the session watcher; Jarvis's memory files, proposals and importer retire.
