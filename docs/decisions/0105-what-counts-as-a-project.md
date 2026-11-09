# 0105 What counts as a project

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
What counts as a project under a watched root?

## Facts
- Measured 2026-10-07: a depth-1 scan of `D:/` found 58 folders, 37 of them git repos, in 7 ms (stat only).
- v4 P8.7 had non-git areas; phase 8 uses areas again.
- Principle 8: minimal by default.

## Options
| Option | Pros | Cons |
|---|---|---|
| Git top-levels found by the scan, plus non-git folders added by hand | The scan only picks real repos | Non-git projects have path identity only |
| Git repos only | One identity rule | No non-git projects; phase 8 areas need another mechanism |
| Every first-level folder | Zero config | 21 non-repo folders on `D:/` alone become projects |

## Recommendation
Git repos plus added folders.

## Choice
The scan registers git top-levels; a non-git folder becomes a project only through `project add`, with path identity.

## Reason
Not given.

## Consequences
- Non-git projects use the `path:` key (0103).
