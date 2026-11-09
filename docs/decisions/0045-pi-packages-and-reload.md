# 0045 Pi packages and /reload

- Status: v1
- Kind: feature
- Phase/slice: 0/0.2
- Vision: V8
- Date: 2026-10-04

## Question
Do Pi's package installs and `/reload` stay?

## Facts
Free with `main()`; the `packages` setting takes npm, git or local sources (Pi `docs/settings.md`, `v1.0.1`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep | Community packages before phase 6; extensions reload themselves | Third-party code with full access before the phase 2 guard |
| Disable | Only compiled-in code runs | Loses `/reload` in development |

## Recommendation
Keep; the fixture gate (0026) limits exposure until phase 2.

## Choice
Keep.

## Reason
Not given.

## Consequences
Installed packages live in Jarvis's config dir (0.3).
