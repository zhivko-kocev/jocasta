# Editors demo (U, 0393, 0395)

Same code and the same diff in the JARVIS palette, rendered by the two candidate editors, so the choice can be made by looking (0393: "we should see how it would look like first").

| Screenshot | Shows |
|---|---|
| `monaco-code.png` | monaco-editor 0.57.0, `monaco.editor.create`, read-only: `src/shared/yaml.ts` lines 1–40 |
| `monaco-diff.png` | monaco-editor 0.57.0, `monaco.editor.createDiffEditor`, side by side, unchanged regions hidden: the 3-hunk diff |
| `pierre-code.png` | shiki 4.5.0 through `@pierre/diffs` 1.5.2 `File`, read-only: the same 40 lines |
| `pierre-diff.png` | `@pierre/diffs` 1.5.2 `FileDiff`, `diffStyle: "split"`: the same 3-hunk diff |

Each is a 1280×800 viewport captured at device scale 2 (2560×1600 PNG).

## Input

- Code: lines 1–40 of `src/shared/yaml.ts`.
- Diff: the whole file against a copy with 3 small edits far apart: line 11 (`:${line}:` → `:${line}:1:`), line 63 (`"the file"` → `"the document root"`), and one comment line added after line 144.

## Look

- Page: ink-0 `#06090f` with the faint arc light from above; panel ink-1 `#0a0f17`, hairline border at 16%, 10px corners; header in Mona Sans, path in Geist Mono (DESIGN.md).
- Code: Geist Mono 12.5px, ligatures off, tab size 2. Line height is 18px instead of DESIGN.md's 1.65, so 40 lines fit in 800px.
- Token colours: the canvas code palette (`src/frontier/ui/palette.ts` `CODE`). Pierre uses the shiki theme from `src/frontier/ui/canvas/highlight.ts` as is (background set to ink-1); Monaco uses a `defineTheme` with the same colours on Monaco's token names.
- Diff: additions sage `#9cc9b0`, deletions rose `#e39a9a`, line tint 8%, changed words 20–22%.
- Both load the repo's `GeistMono-VF.woff2` and `MonaSans-VF.woff2` from `src/frontier/ui/fonts/`.

## What differs on screen

- **Highlighting.** Monaco's built-in TypeScript tokenizer (Monarch) doesn't mark function names or calls, so `parseYamlFile`, `split`, `locate` stay ink. Shiki uses the TextMate grammar and lights them cyan, as the canvas does today. Monaco gets the same grammar only by adding a TextMate engine (not tried here).
- **Code view.** Monaco shows indent guides and is a full editor (find, folding, selection, cursor). Pierre's `File` is rendered HTML in a shadow root, styled through `--diffs-*` CSS variables.
- **Diff view.** Monaco: `−`/`+` gutter marks, char-level emphasis, "N hidden lines" bars that expand, diff computed in a web worker. Pierre: file header with `−2 +3`, bar indicators, word-level emphasis, "N unmodified lines" separators that expand, no worker needed (an optional worker pool exists).
- Pierre 1.5.2 also has an edit mode (`edit`, `onEditChange`); not used here.

## Bundle sizes (measured)

Built with `bun build` (Bun 1.4.2, `target: "browser"`, `minify`, `splitting`). The repo fonts are left out of the counts (Geist Mono 71,596 B and Mona Sans 307,976 B, the same for both). gzip is level 6, for comparison only; the companion loads from disk.

**Shipped** = every file in the page's output folder.

| Option | Files | JS | CSS | Worker | Other | Total | gzip |
|---|---|---|---|---|---|---|---|
| Monaco | 7 | 3,048,718 | 113,047 | 311,328 | codicon font 153,228; html 394 | **3,626,715** | 966,364 |
| shiki + Pierre | 400 | 10,838,587 | 745 | — | html 329 | **10,839,661** | 2,040,659 |

**Loaded** = bytes the page actually fetched (server log) to show each view.

| Option | Code view | Diff view |
|---|---|---|
| Monaco | 3,315,387 (gzip 869,689); no worker | 3,626,715 (gzip 966,364); with `editor.worker.js` |
| shiki + Pierre | 732,458 (gzip 172,949) | 732,458 (gzip 172,949) |

- Monaco's JS: main chunk 2,888,215, entry 155,598, lazy TypeScript tokenizer 4,905. Imports are trimmed to what a viewer needs: `editor.api`, the diff editor, find, folding, the TypeScript tokenizer and codicons. `import * as monaco from "monaco-editor"` (every language, the TS language service) would be larger; not measured.
- Pierre's shipped size is almost all lazy chunks: `@pierre/diffs` imports the full `shiki` bundle, so every grammar and theme is emitted though only TypeScript loads (core chunk 550,255 + TypeScript grammar 181,072). Limiting the emitted grammars to the canvas's list in `highlight.ts` would need a resolver plugin; not measured.

## How it was made

Scratch folder `%TEMP%\jocasta-editors-demo\` (nothing installed in the repo):

1. `bun add monaco-editor@0.57.0 shiki@4.5.0 @pierre/diffs@1.5.2`.
2. `src/theme.ts` (palette, shiki theme), `src/monaco.ts` + `monaco.html`, `src/pierre.ts` + `pierre.html`, `src/page.css`; the inputs as `code40.txt`, `before.txt`, `after.txt`. `?view=code|diff` picks the view, so each option is one bundle.
3. `bun build.ts`: one HTML entry per option into `dist/<option>/`, plus Monaco's `editor.worker.js` as its own entry; stylesheets the HTML doesn't link (a duplicate Bun emits for the split chunk) are dropped before counting.
4. `bun serve.ts`: serves `dist/` and the fonts on 127.0.0.1, logging the bytes of each response.
5. Headless Chrome 155 (Puppeteer through omp's browser tool), viewport 1280×800, device scale 2. Each page sets `data-ready` once rendered (for Monaco's diff, after the worker returns the diff); the capture waits for it. The plain `chrome --headless --screenshot` capture fired before Monaco's worker had finished, so it wasn't used.
