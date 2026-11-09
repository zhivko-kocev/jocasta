# 0314 Self-extension prompt line

- Status: decided
- Kind: product
- Phase/slice: 1/1.11
- Date: 2026-10-08

## Question
Does Jarvis's prompt section point the model at Jarvis's extensions folder?

## Facts
- 0175: asked for a feature, Jarvis writes an extension into `~/.jarvis/agent/extensions/` and loads it with `/reload`. Pi's shipped docs name `~/.pi/agent/extensions/`, and 0172's section doesn't name Jarvis's folder.

## Options
| Option | Pros | Cons |
|---|---|---|
| Add one line | The model writes where Pi's loader looks | One prompt line |
| No line | — | Extensions likely land in the wrong folder |

## Recommendation
Add the line.

## Choice
Jarvis's prompt section (0172) gains: `- Extending Jarvis: write extensions to ~/.jarvis/agent/extensions/ (Pi's docs say ~/.pi/agent; use Jarvis's), then /reload.`

## Reason
Not given.

## Consequences
- Amends 0172's section text.
