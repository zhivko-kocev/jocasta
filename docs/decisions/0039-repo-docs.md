# 0039 Repo docs

- Status: decided
- Kind: product
- Phase/slice: 0/0.1
- Date: 2026-10-04

## Question
Which docs does the repo carry?

## Facts
`AGENTS.md` already exists (2026-10-04) as the rules that keep agents on the plan. The repo was private when asked; it is now public (0035).

## Options
| Option | Pros | Cons |
|---|---|---|
| README, AGENTS.md (package map and conventions for agents), CONTRIBUTING.md, an MIT LICENSE and a CHANGELOG | Complete if the repo ever opens; the CHANGELOG feeds releases (0.2) | LICENSE and CONTRIBUTING matter little while private |
| README and AGENTS.md only | Minimal | The rest is added later |

## Recommendation
The full set if public; README and AGENTS.md if private.

## Choice
The full set: README, AGENTS.md, CONTRIBUTING.md, an MIT LICENSE and a CHANGELOG.

## Reason
Not given.

## Consequences
Matches the recommendation now that the repo is public (0035). AGENTS.md gains the package map and conventions alongside its drift rules.
