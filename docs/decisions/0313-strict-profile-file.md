# 0313 Strict profile file

- Status: decided
- Kind: product
- Phase/slice: 1/1.12
- Date: 2026-10-08

## Question
How does the strict profile ship, what does it hold, and how do profiles layer?

## Facts
- 0186: strict pins no model fallback, no account switching, no background summaries and a fixed effort. Building 1.12 found only one decided key: `providers.rotateAccounts` (0311). Model roles land in 6.2 (0283); Pi has no background summaries; the seeded thinking level is `high` (0056).
- As built, a profile layers over Pi's settings with precedence profile > project `.jarvis/settings.json` > global `settings.json`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Seed `profiles/strict.yaml` once if missing, as previewed | Editable, like `settings.json` (0056) | — |
| Built in, not a file | Can't drift | Not editable |
| Seed it with effort `medium` | Cheaper | Weaker |

## Recommendation
Seed once, as previewed.

## Choice
Now (0377): the file holds `defaultThinkingLevel: high` and `modelFallback: false`; account rotation is gone.

Originally: `~/.jarvis/agent/profiles/strict.yaml` is written once when missing, then it is his. Precedence: profile > project `.jarvis/settings.json` > global `settings.json`.

```yaml
defaultThinkingLevel: high
modelFallback: false
providers:
  rotateAccounts: false
```

## Reason
Not given.

## Consequences
- Amends 0186: no background summaries needs no key (Pi has none).
- `modelFallback` is read by 6.2's role router (0283).
