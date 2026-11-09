# 0408 Keys: Alt digits for workspaces

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Which keys move him around herdr?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).
- herdr's defaults use the `ctrl+b` prefix; `keys.switch_workspace`, `keys.goto`, `keys.next_agent`, `keys.previous_agent` are configurable; `ctrl+alt+]` is omp's.

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr defaults | Nothing to learn beyond herdr | Two keys for everything |
| Alt digits for workspaces | One key to the boss and each project | Alt chords taken |
| herdr's ctrl+alt set | Prefix-free | Clashes with omp and AltGr |
| Ctrl+1–9 | As the old pills | Unverified in Windows Terminal |

## Recommendation
Alt digits.

## Choice
Keep `ctrl+b`; add `switch_workspace = ["prefix+shift+1..9", "alt+1..9"]` (Alt+1 = Jarvis), `goto = ["prefix+g", "alt+g"]`, `next_agent = "alt+]"`, `previous_agent = "alt+["`. Right Ctrl stays the companion's dictation key.

## Reason
"Alt digits for workspaces".

## Consequences
- Amends 0370's Ctrl+1–9.
- Amended by 0554: Ctrl+B, then Alt+F/S/R/N run Finish, Stop, Review and New thread; the Alt keys stay.
