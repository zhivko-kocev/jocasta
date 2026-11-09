# 0406 hunk opens beside the thread

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Where does hunk open when a thread finishes?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).
- hunk keeps split view at ≥ 120 columns (`hunk-src/packages/hunk/src/lib/responsive.ts:5-8`); a 1920 px screen gives herdr about 184 columns.

## Options
| Option | Pros | Cons |
|---|---|---|
| Beside, half width | Both visible; zoom for split view | Unified view at half width |
| Under the thread | Full-width split view | Half height |
| Own tab | Full screen | Thread out of sight |
| Beside, hunk wider | Split view | Narrow thread |

## Recommendation
Beside, half width.

## Choice
hunk opens to the right of the thread pane at half width; `prefix+z` zooms it for split view. Revisited after use.

## Reason
"lets start with recomended but with usage we will see what will be the best".

## Consequences

