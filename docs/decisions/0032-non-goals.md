# 0032 Non-goals for v1

- Status: decided
- Kind: process
- Phase/slice: 0/0.0
- Date: 2026-10-04

## Question
Is the plan's list of v1 non-goals (plan section 4) approved as one record?

## Facts
The proposed list: no source fork of Pi and no Go rewrite (0009); no providers of Jarvis's own; no model-judged "auto" behavior where a fixed rule works; no hosted service that reads client data, and no share links unless 0.2 keeps them; no marketplace of Jarvis's own; no telemetry.

## Options
| Option | Pros | Cons |
|---|---|---|
| Approve the list as one record | One fence later slices check against | Commits before each slice's facts are in |
| Confirm each non-goal in the slice it touches | Each non-goal gets that day's facts | No up-front fence until those slices |

## Recommendation
Approve the list as one record.

## Choice
Confirm each non-goal in the slice it touches. There is no v1 non-goals list; plan section 4's list stays as proposals.

## Reason
Not given.

## Consequences
- Differs from the recommendation.
- Each non-goal becomes a question in the slice it touches: fork and Go rewrite (0009 already holds), providers (0.2, 0.4), "auto" behavior (1.12), hosted services and share links (0.2), marketplace (0.2), telemetry (0.2).
