# 0075 When the container manager is built

- Status: decided
- Kind: process
- Phase/slice: 0/0.7
- Date: 2026-10-04

## Question
When is the container manager built?

## Facts
Plan appendix I lists the container manager as a phase 0 seam, but no phase 0 slice adds a resource; the first is 4.8.

## Options
| Option | Pros | Cons |
|---|---|---|
| With the first resource (4.8) | No code without a user | Appendix I moves |
| Now, with no resources | Seam exists early | Nothing to run |

## Recommendation
With the first resource.

## Choice
With the first resource that needs it (4.8 today). Phase 0 records the rules only.

## Reason
Not given.

## Consequences
Plan appendix I's container-manager row moves from phase 0 to the first slice that adds a resource.
