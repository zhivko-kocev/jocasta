# 0062 Anthropic sign-in route

- Status: decided
- Kind: product
- Phase/slice: 0/0.4
- Date: 2026-10-04

## Question
How does Jarvis reach Anthropic models?

## Facts
- Pi 1.0.1 (`interactive-mode.js`, setting `warnings.anthropicExtraUsage`, default on): "Anthropic subscription auth is active. Third-party harness usage draws from extra usage and is billed per token, not your Claude plan limits."
- Claude Code docs, "Legal and compliance" (https://code.claude.com/docs/en/legal-and-compliance, read 2026-10-04): OAuth "is intended exclusively for" subscription purchasers "to support ordinary use of Claude Code and other native Anthropic applications"; developers "should use API key authentication"; signing in to the unmodified Claude Code binary is allowed; "Anthropic reserves the right to take measures to enforce these restrictions".
- Pi's OAuth login identifies itself as Claude Code on the wire (plan 5.6).

## Options
| Option | Pros | Cons |
|---|---|---|
| Anthropic API key as the supported route | Explicitly permitted; predictable billing | Per-token cost |
| Subscription OAuth through Pi | One login | Billed as extra usage per Pi; against the stated intent; enforcement risk |
| `claude -p` as a provider | Permitted subscription use | A provider to build; Jarvis's tools and guard don't apply inside it |
| ChatGPT/Codex OAuth as primary | Subscription-based | OpenAI's policy unverified |

## Recommendation
Anthropic API key.

## Choice
Subscription OAuth through Pi's `/login`, one account. Now (0377): `@gotgenes/pi-anthropic-auth` shapes the requests, and GitHub Copilot comes first in the seeds and roles. Earlier (0337): Jarvis's own shaping; (0336): the package.

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- Accepted with today's facts: usage may bill as Anthropic extra usage per token, and Anthropic may enforce its OAuth terms without notice.
- Pi's extra-usage warning stays on (Pi's default).
- Plan appendix E's contingency applies if Anthropic rejects Pi's OAuth: re-grill with the API key and `claude -p` routes.
