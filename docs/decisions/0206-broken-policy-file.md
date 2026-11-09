# 0206 Broken policy file

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
What happens when `permissions.yaml` is broken?

## Facts
- Principle 4: a broken guard blocks and says why.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Block every guarded call with a message naming the file and line | Fails closed | He fixes it in an editor |
| Ignore the file | Keeps working | Fails open |

## Recommendation
Block.

## Choice
A `permissions.yaml` that doesn't parse or has an unknown key blocks every guarded call with a message naming the file and line; he fixes it in an editor.

## Reason
Not given.

## Consequences
- None.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
