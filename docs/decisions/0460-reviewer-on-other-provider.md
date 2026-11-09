# 0460 The reviewer runs on the other provider

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
The review step's reviewer runs on the same vendor as the thread it reviews. Which model reviews?

## Facts
Verified 2026-10-23.
- omp's reviewer agent runs `model: "@slow"` (`prompts/agents/reviewer.md:6`); Jarvis's review prompt asks for "the reviewer subagent" with no model (`hunk.ts:84-86`).
- omp routes agents per role: `task.agentModelOverrides.reviewer` (https://omp.sh/docs/agents-and-roles); a user `~/.omp/agent/agents/reviewer.md` overrides the bundled one (`task/discovery.ts:76-81`).
- Same-model self-review shares the author's blind spots (https://raw.githubusercontent.com/shanraisshan/claude-code-best-practice/main/development-workflows/cross-model-workflow/cross-model-workflow.md); OpenAI's codex-plugin-cc review prompt says "Report only material findings" and "Prefer one strong finding over several weak ones" (`plugins/codex/prompts/adversarial-review.md`). Pattern only; no measured gain in either source.
- A Copilot request whose last message is the user's costs a premium request at the model's multiplier (omp `provider-quirks.md` "GitHub Copilot") [INFERENCE].

## Options
| Option | Pros | Cons |
|---|---|---|
| A GitHub Copilot GPT reviewer, falling back to omp's default | A separate vendor's view; uses a login he has | Copilot premium requests; needs the login |
| omp's default (`@slow`) | No change | Shares the author's blind spots |
| `modelRoles.review` at high effort | Cheaper reviews | Same vendor; he rejected it (0451) |

## Recommendation
A GitHub Copilot GPT reviewer, falling back to omp's default.

## Choice
The review step's reviewer runs on a GitHub Copilot GPT model while threads work on Anthropic (separate vendor), set in the per-project overlay (`task.agentModelOverrides.reviewer` or omp's equivalent; builder verifies the key); with no Copilot login it falls back to omp's default. The review prompt adds "report only material findings; prefer one strong finding over several weak ones" (from OpenAI's codex-plugin-cc review prompt).

## Reason
"A GitHub Copilot GPT reviewer, falling back to omp's default".

## Consequences
- Amends 0425 (the reviewer's model and prompt).
- The builder verifies the override key and the model, and lists them as built.
- Amended by 0488: Jocasta sets no reviewer model; omp's bundled reviewer runs on his `slow` role.
