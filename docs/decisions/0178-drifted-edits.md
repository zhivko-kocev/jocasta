# 0178 Drifted edits

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-07

## Question
What happens to an edit whose target moved?

## Facts
- Hashline (0146) cites line tags, never text; a stale tag is refused.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fail loudly | No wrong applies | A re-read costs a turn |
| Fuzzy matching | Fewer refusals | Wrong applies |

## Recommendation
Fail loudly.

## Choice
An edit whose tags no longer match fails loudly; met by hashline (0146).

## Reason
Not given.

## Consequences
- Appendix G's lever is decided.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
