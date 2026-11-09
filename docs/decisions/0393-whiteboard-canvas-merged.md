# 0393 whiteboard's canvas, merged

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
What draws on the canvas?

## Facts
Verified 2026-10-13.
- whiteboard (`devdotfast/whiteboard`) v0.2.4 (2026-10-09), MIT, a pnpm 11 + Node 24 monorepo with an Electron + Code-OSS desktop. Its canvas `@dev.fast/review-canvas` is private (`packages/review/app`, build `dist/desktop`, `mountReviewCanvas(container, content, ui)`, host bridge `ReviewCanvasBridge` with `inlineEditors` and `diffView`); its runtime `@dev.fast/whiteboard` has `whiteboard server` and `whiteboard mcp`; state in `DEV_REVIEW_SERVER_DIR` or `DEV_REVIEW_HOME`, discovery `<stateDir>/review-server/server.json`; telemetry off with `DO_NOT_TRACK=1`.
- Jarvis's canvas copies whiteboard's look (0379) on React Flow + ELK; `draw` is its heaviest tool (1,867 tokens).
- whiteboard's code peek lives in its Code-OSS shell; its canvas asks its host for `inlineEditors` and `diffView`.

## Options
| Option | Pros | Cons |
|---|---|---|
| whiteboard's canvas merged into the companion | whiteboard's full canvas and MCP tools; Jarvis's draw code goes | Node 24 + pnpm build; a bridge to write |
| whiteboard app beside Frontier | No build | A second 162 MB Electron app outside Jarvis |
| Keep Jarvis's canvas | As built | A copy that trails whiteboard |

## Recommendation
Merge whiteboard's canvas.

## Choice
whiteboard v0.2.4 is built from source (submodule, no patch); its canvas mounts in the companion through a Jarvis `ReviewCanvasBridge`; its server runs as the companion's child with telemetry off and state in `<data>/whiteboard`; agents draw through `whiteboard mcp`; Jarvis's `draw` and canvas retire. Editors are chosen after the look-first demo in the UX pass (0395).

## Reason
"whiteboard's canvas merged into Frontier"; "we should see how it would look like first".

## Consequences
- Needs Node 24 and pnpm 11 (0385).
- A source change needed to mount outside Code-OSS forks whiteboard with the smallest patch (0383).
- Amended by 0552: no companion app; whiteboard's canvas opens in his browser from whiteboard's server.
