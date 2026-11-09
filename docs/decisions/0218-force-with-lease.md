# 0218 Force push with lease

- Status: decided
- Kind: product
- Phase/slice: 2/2.2
- Date: 2026-10-08

## Question
Is `git push --force-with-lease` denied like a plain force push?

## Facts
- 0210 recorded force push "including `--force-with-lease`" from a multi-select whose label ended in a question mark, so it was re-asked (0020).
- `--force-with-lease` refuses to overwrite remote commits that haven't been fetched; rebasing a pushed feature branch needs it.
- His own `!` commands meet deny rules but not asks (0202).

## Options
| Option | Pros | Cons |
|---|---|---|
| Deny it too | Simplest rule | Rebased branches are pushed from a terminal |
| `--force-with-lease` asks; `--force`, `-f` and `+refspec` deny | Rebases still flow | Two rules |

## Recommendation
Lease asks.

## Choice
`--force-with-lease` asks in every mode (so his own `!` command runs); `--force`, `-f` and `+refspec` are denied in every mode.

## Reason
Not given.

## Consequences
- Amends 0210: force push with lease moves from deny to ask.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
