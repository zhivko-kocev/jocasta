# 0074 Container images

- Status: decided
- Kind: product
- Phase/slice: 0/0.7
- Date: 2026-10-04

## Question
Which images, and how are they pinned?

## Facts
Pulling or building an image counts as an install and is asked first (plan section 4).

## Options
| Option | Pros | Cons |
|---|---|---|
| Official images, pinned by digest; Jarvis's own from Dockerfiles in the repo | Reproducible | Digest bumps by hand |
| Latest tags | No bump work | Unreproducible |

## Recommendation
Pinned by digest.

## Choice
Official images where they exist, pinned by digest; Jarvis's own images built from Dockerfiles in the repo.

## Reason
Not given.

## Consequences
A digest bump is its own change.
