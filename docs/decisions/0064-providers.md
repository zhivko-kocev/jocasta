# 0064 Providers

- Status: decided
- Kind: product
- Phase/slice: 0/0.4
- Date: 2026-10-04

## Question
Which providers does Jarvis offer?

## Facts
Pi 1.0.1 ships API-key, OAuth, Bedrock, Vertex and llama.cpp providers (llama.cpp kept in 0046). Restricting needs code that filters Pi's list.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep Pi's full list; the docs name the supported routes | No code | Unsupported routes one `/login` away |
| Restrict to the supported routes | Nothing unsupported to pick | Filtering code against Pi internals |

## Recommendation
Keep Pi's list.

## Choice
Keep Pi's full list. The supported Anthropic route is subscription OAuth (0062).

## Reason
Not given.

## Consequences
No Jarvis provider code (plan section 4 non-goals: no providers of Jarvis's own, confirmed here per 0032).
