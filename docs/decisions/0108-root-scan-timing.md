# 0108 When roots are scanned

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
When are watched roots scanned?

## Facts
Measured 2026-10-07 on this machine.
- Listing `D:/` at depth 1 takes 7 ms; resolving identity takes about 60 ms per git call, so 37 repos with two calls each took 4.4 s.
- What changed (0004): the plan's first option scanned at startup; re-resolving every repo on each launch would add seconds.

## Options
| Option | Pros | Cons |
|---|---|---|
| At startup, listing the roots and running git only for paths the catalog hasn't seen or whose `.git/config` changed; live watching from phase 4 | New clones appear on the next launch at almost no cost | An origin changed without touching `.git/config` is missed |
| On demand only | No startup work | A new clone is invisible until a rescan |
| A full scan at every startup | Always exact | About 4.4 s per launch today |

## Recommendation
At startup with cached identity. This changes the plan's first option because of the measurement above.

## Choice
Every launch lists the watched roots and runs git only for new paths or a changed `.git/config`; Frontier watches the roots live from phase 4.

## Reason
Not given.

## Consequences
- There is no rescan command (0111): re-resolution relies on `.git/config` change detection.
