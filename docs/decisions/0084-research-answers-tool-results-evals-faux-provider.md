# 0084 Research answers: tool results in the session file, Pi's evals, Pi's faux provider

- Status: decided
- Kind: process
- Phase/slice: 0/5.4
- Date: 2026-10-07

## Question
Does the plan take three answers found in the research?

## Facts
Verified 2026-10-07.
- Plan 5.4 asked for phase 2 whether `tool_result` edits land before the session file is written. The learn-harnesses L69 reference run on Pi 1.0.1 (2026-10-06) stored the hooked (spilled) result in the session file, not the 1 MiB output (`t14-capstone/LESSON-69-jarvis-tool-overrides-prototype.md:344-350`). Pi's `createToolResultMessage` copies `content`, `details` and `usage`, not `structuredContent` (`packages/agent/src/agent-loop.ts:922-935` at v1.0.1).
- Plan 1.10 said "The slice checks whether Pi's `packages/evals` fits." `@earendil-works/pi-evals` 1.0.1 is `"private": true` and built on `vitest-evals` (`packages/evals/package.json:2-4`, `README.md:3`); Jarvis's tests run on Bun (0036).
- `@earendil-works/pi-ai` 1.0.1 (MIT), installed in the Jarvis repo, exports a scripted faux provider: `fauxProvider()`, `fauxText`, `fauxThinking`, `fauxToolCall`, `fauxAssistantMessage`, response factories, simulated usage and prompt-cache reads (`dist/index.d.ts:22`, `dist/providers/faux.d.ts`).
- What changed for 1.10 (0004): the planned fake provider was Jarvis's own, in v4 P1.7-10's format; Pi now ships one.

## Options
| Option | Pros | Cons |
|---|---|---|
| All three | The list shows what is known; 1.10 reuses Pi's provider instead of writing one | Phase 1 and 2 fact checks still re-run both at the pinned versions |
| Research list only | 1.10's options stay as planned | 1.10 may write a provider Pi already ships |
| Neither | No change | Answered questions stay open |

## Recommendation
All three.

## Choice
All three.

## Reason
Not given.

## Consequences
- Plan 5.4: the phase 2 item is marked answered; phase 2's fact check re-runs it at the pinned Pi.
- Plan 1.10: the fake provider's first option is Pi's faux provider, registered by a Jarvis extension and fed by a script file; the `packages/evals` note becomes the fact that it is a design reference only.
- Appendix B gains Pi's faux provider (candidate, 1.10) and Pi's evals package (reference, 1.10).
