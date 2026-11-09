# 0107 Project ids

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
What form do project ids take?

## Facts
- Plan 1.2 (v4 F0 slice 4): `p-` + 16 hex.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| `p-` plus the first 16 hex of SHA-256 of the identity key | The same repo has the same id on every machine; 2^64 space | Ids change if the identity key changes |
| Random ids | Never change | Differ across machines |

## Recommendation
`p-` + 16 hex from the identity key.

## Choice
`p-` plus the first 16 hex digits of SHA-256 of the identity key (0103), for example `p-1a2b3c4d5e6f7a8b`.

## Reason
Not given.

## Consequences
- Phase 9 sync can match projects across machines by id.
