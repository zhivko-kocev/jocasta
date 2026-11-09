# 0051 Releases

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
How are releases built?

## Facts
- Private repos run Actions on the account's free minute quota (docs.github.com, "GitHub Actions billing"); public repos on standard runners are free.

## Options
| Option | Pros | Cons |
|---|---|---|
| GitHub Actions on a tag: build each target, package, checksums, GitHub release | Reproducible; install scripts have a source | Minutes if private |
| Local builds only | Free | Nothing for the install scripts to fetch |

## Recommendation
Actions on a tag.

## Choice
GitHub Actions on a tag: build the three targets (0043), package (0044), write checksums, publish a GitHub release.

## Reason
"then the repo will be public"

## Consequences
Made the repo public (0035, edited).
