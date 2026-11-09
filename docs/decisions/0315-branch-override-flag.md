# 0315 Branch override flag

- Status: decided
- Kind: product
- Phase/slice: 1/1.6
- Date: 2026-10-08

## Question
What is the flag that overrides a new workspace thread's branch name?

## Facts
- 0121: naming a branch when a thread is created overrides `feature/<slug>`; the flag name is decided with the `thread` commands, built as `jarvis thread new "<title>" --brief "<brief>"`.

## Options
| Option | Pros | Cons |
|---|---|---|
| `--branch <name>` | Plain | — |
| `-b <name>` and `--branch <name>` | Short form | Two spellings |

## Recommendation
`--branch <name>`.

## Choice
`jarvis thread new "<title>" --brief "<brief>" --branch <name>`, also on `/thread new`.

## Reason
Not given.

## Consequences
- Appendix I's `thread` row gains the flag.
