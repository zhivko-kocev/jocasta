# 0385 Installs for the upstreams

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-13

## Question
Which installs does he approve, each only when its stage starts?

## Facts
Verified 2026-10-13.
- herdr (`herdrdev/herdr`) 0.9.4 (2026-10-13), Apache-2.0, Rust; Windows is GA through ConPTY, `herdr terminal attach` is unsupported there. It has an NDJSON socket API on a named pipe (`workspace.*`, `tab.create`, `pane.*`, `agent.*`, `worktree.*`, `notification.show`, `events.subscribe`), env `HERDR_SESSION` and `HERDR_CONFIG_PATH`, and `herdr integration install` supports OMP on Windows.
- whiteboard (`devdotfast/whiteboard`) v0.2.4 (2026-10-09), MIT, a pnpm 11 + Node 24 monorepo with an Electron + Code-OSS desktop. Its canvas `@dev.fast/review-canvas` is private (`packages/review/app`, build `dist/desktop`, `mountReviewCanvas(container, content, ui)`, host bridge `ReviewCanvasBridge` with `inlineEditors` and `diffView`); its runtime `@dev.fast/whiteboard` has `whiteboard server` and `whiteboard mcp`; state in `DEV_REVIEW_SERVER_DIR` or `DEV_REVIEW_HOME`, discovery `<stateDir>/review-server/server.json`; telemetry off with `DO_NOT_TRACK=1`.
- Rust is needed only for a herdr or omp natives patch.

## Options
| Option | Pros | Cons |
|---|---|---|
| herdr 0.9.4 | The work surface (0392) | A new binary on the machine |
| Node 24 + pnpm 11 | Builds whiteboard from source (0393) | Two toolchains |
| Rust via rustup | Only for a herdr patch | Large toolchain |
| whiteboard 0.2.4 desktop | Its full app for comparison | 162 MB Electron app |

## Recommendation
Approve each when its stage starts.

## Choice
Approved when their stage starts: herdr 0.9.4, Node 24 + pnpm 11, Rust via rustup, the whiteboard 0.2.4 desktop app (approved, unused).

## Reason
His ticks on all four.

## Consequences
- Plan section 4's install rule lists these as approved.
- Anything else still needs his OK.
- Amended by 0435: herdr 0.9.5 (`herdr update`).
