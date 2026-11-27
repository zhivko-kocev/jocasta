# 0705 Ponytail text refreshed

- Status: v1 (built 2026-11-27)
- Kind: product, prompts
- Phase/slice: ultron 0.3 iteration, J10
- Date: 2026-11-26

## Question
`config/ponytail.md` is Ponytail 5.2.0's `AGENTS.md` form with 0612's three edits. Upstream has moved on. Does Jocasta take its current text?

## Facts
- Verified 2026-11-27: upstream's latest release is 5.3.3 (`package.json`); `AGENTS.md` on `main` is at commit `ebd8654` (2026-11-22).
- Its text differs from Jocasta's file only in 0612's three lines (the comment bullet, the structure and diff bullets made one, the line about the ponytail repo itself); its MIT licence text is unchanged.

## Options
| Option | Pros | Cons |
|---|---|---|
| Take upstream's current `AGENTS.md`, re-applying 0612's three edits | Jocasta's copy follows upstream | A future upstream change may need its own look |
| Keep 5.2.0 | Nothing to do | The header names an old release |

## Recommendation
Take upstream's current text.

## Choice
His answers of 2026-11-26 and the approved plan: `config/ponytail.md` is upstream's current `AGENTS.md` with 0612's three edits and Jocasta's "Running tests" section; its header names the release (5.3.3) and the commit it was read at.

## Reason
Keep the always-on text in step with upstream.

## Consequences
- 2026-11-27: only the header changed; the prompt text is 5.2.0's, so the eval variants' copies and every pane's prompt are unchanged.
