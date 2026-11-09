# 0411 Canvas editors: Monaco

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Which editors host whiteboard's code peeks and diffs?

## Facts
Verified 2026-10-13.
- Demo: `docs/research/editors-demo/` (screenshots, measured bundles).
- Monaco 0.57.0 loads 3.6 MB for a diff with its worker, has find, folding and the diff editor built in; shiki 4.5.0 + @pierre/diffs 1.5.2 loads 0.73 MB and colours function names, but find, viewed marks and the file tree would be Jarvis code.

## Options
| Option | Pros | Cons |
|---|---|---|
| Monaco | Whiteboard's own host editor; least Jarvis code | Heavier |
| shiki + @pierre/diffs | Lighter; hunk's look | More Jarvis code |

## Recommendation
Monaco.

## Choice
Monaco 0.57.0 for `inlineEditors` and `diffView`, themed Tokyo Night (0401).

## Reason
"Monaco".

## Consequences
- `monaco-editor` 0.57.0 joins the dependencies at F3; CSP adds `worker-src 'self'`.
