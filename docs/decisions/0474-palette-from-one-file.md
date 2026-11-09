# 0474 Tokyo Night colours from one file

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
The Tokyo Night hex values are copied by hand into several files. Do they come from one source?

## Facts
Verified 2026-10-23.
- The hex values are copied by hand in `src/frontier/ui/styles.css`, `src/frontier/ui/palette.ts`, `src/frontier/ui/whiteboard/monaco.ts`, the Windows Terminal scheme in `src/setup/setup.ts`, `src/frontier/ui/index.html` and `DESIGN.md` (`local://omarchy-OmarchyRepo.md`).
- Omarchy keeps one `colors.toml` per theme and renders each app's file from templates (https://github.com/omacom/omarchy/blob/quattro/docs/theming.md, lines 65-173).

## Options
| Option | Pros | Cons |
|---|---|---|
| One palette file; the others read or are generated from it | Colours can't drift; no visible change | A small build step |
| Keep the copies | No work | Every change touches six files |

## Recommendation
One palette file; the others read or are generated from it.

## Choice
One palette file in the repo is the source; the others read or are generated from it (DESIGN.md names it). No visible change.

## Reason
"Tokyo Night colours from one file instead of six copies".

## Consequences
- Amends 0401 (its Tokyo Night colours come from the one file).
- A theme switcher is not part of this.
