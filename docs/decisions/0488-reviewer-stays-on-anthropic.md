# 0488 The reviewer stays on Anthropic

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
0460 put the review step's reviewer on a GitHub Copilot GPT, a different vendor from the Anthropic threads. His omp has no Copilot login, so every review already runs on Anthropic. Does he log in to Copilot, or do reviews stay on Anthropic?

## Facts
Verified 2026-10-24.
- His omp's `agent.db` holds two credentials, both `anthropic` OAuth; none for `github-copilot`.
- The overlay's `task.agentModelOverrides.reviewer: ["github-copilot/gpt-6.1-sol", "@slow"]` (`service.ts:222`) resolves to its second pattern without a Copilot login: in the sandbox walkthrough the weekly review's reviewer ran on `anthropic/claude-opus-5-5` at `max` (its session's `model_change`).
- omp's bundled reviewer already declares `model: "@slow"` (`prompts/agents/reviewer.md`); his `slow` role is `anthropic/claude-opus-5-5:max`.
- The reviewer works from a fresh context either way: it sees the plan, the rules, the lessons and the diff, not the thread's conversation (0453).
- Benchmark arm A3 (0482) measured the review on Copilot.

## Options
| Option | Pros | Cons |
|---|---|---|
| He logs in to Copilot in omp | A reviewer from another vendor, as 0460 decided; A3 measures it | Another login to keep; his Copilot subscription |
| Anthropic only: reviews on Opus 5.5 | Nothing to log in to; one vendor to pay | Same vendor as the threads; a fresh context is the only independence |

## Recommendation
He logs in to Copilot in omp.

## Choice
Reviews run on Anthropic. Jocasta sets no reviewer model: the project overlay drops `task.agentModelOverrides`, so omp's bundled reviewer runs on its own `@slow`, his `slow` role (today `anthropic/claude-opus-5-5:max`). The review prompt keeps "report only material findings; prefer one strong finding over several weak ones" (0460). Benchmark arm A3 pins its reviewer to the arms' model, `anthropic/claude-opus-5-5`, so it measures a same-vendor review in a fresh context.

## Reason
"Anthropic only: reviews on Opus 5.5" (differs from the recommendation).

## Consequences
- Amends 0460 (the reviewer's model) and 0482 (A3's reviewer; no Copilot login needed for the benchmarks).
- The thesis reports the review as a same-vendor, fresh-context reviewer.
