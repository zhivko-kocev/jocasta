# 0047 /share to a GitHub gist

- Status: v1
- Kind: feature
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
Does `/share` stay, and how is it guarded?

## Facts
- Pi tag `v1.0.1` (github.com/earendil-works/pi), read 2026-10-04: `/share` is hard-coded in `interactive-mode.ts`. An extension command with the same name doesn't override it; Pi skips or renames the extension's command ("conflicts with built-in interactive command").
- A custom editor component could swallow `/share` before Pi sees it; that is unverified and depends on editor internals.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep, with a docs warning, and open an upstream PR for a setting that disables it | Clean if Pi merges | Open until merged |
| Intercept through a custom editor component | Blocks the upload | Fragile across Pi bumps |
| Keep, with a docs warning only | Nothing to maintain | One command from uploading client data |

## Recommendation
Keep with a docs warning, and open the upstream PR.

## Choice
Keep `/share`, with a warning in the docs. No upstream PR.

## Reason
Not given.

## Consequences
Differs from the recommendation: no PR is opened. The README warns that `/share` uploads the whole thread to a GitHub gist.

- Amended by 0386: Pi's gist `/share` is gone with Pi; omp's `/share` uploads an encrypted blob to `my.omp.sh/s` by default, a gist only with `--gist`.
