# 0679 Whiteboard and hunk removed; `show` draws a brief

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Amends: 0383, 0394, 0607, 0615
- Date: 2026-11-24

## Question
With briefs (0678), do whiteboard (canvas, Node) and hunk stay?

## Facts
- Whiteboard's `node.exe … desktop-host.js` (41 MB) outlived Jocasta's Stop by hours.
- Of Jocasta's `TOOLS` only the boss's `show` touches the canvas (0607).
- He left the free-hand drawing call to the builder.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep both | No removal work | Extra processes and a drawing surface the model must operate |
| Remove both; `show` draws a brief from a spec | One less runtime (Node), less code, deterministic output | No free-hand drawing: a change to a diagram is a new `show` |

## Recommendation
Remove both; no free-hand surface.

## Choice
His answer of 2026-11-24 (free-hand drawing left to the builder, who drops it):
- The boss's `show` takes `{title, sections}` with section types `text`, `facts`, `flow`, `map`, `diff`, `list` only; a layout validation error (duplicate id, edge to an unknown id, more than 200 nodes) is the tool's error text; success stores `show-<n>`, opens it and answers `Shown: <title>`.
- Deleted: `whiteboard.rs`, `src/frontier/canvas/`, canvas routes, `whiteboard_connection`, `whiteboard_reviews`, `guarded_stage`, the whiteboard row of `services_row`, its stop in `stop_sequence.rs`, whiteboard and Node pieces of setup and `install.rs`, the `forks/whiteboard` submodule, the canvas test suites and entries in `knip.jsonc`, `package.json`, `scripts/build.ts`; whiteboard's tool server leaves the boss's config.
- Hunk: `hunk.rs`, `fake_hunk.rs`, `PaneLaunch::hunk`, `ensure_hunk_daemon`, `end_hunk_daemon`, `hunk_port`, `Herdr::split_beside` and the `hunk/` folder of the build and installers; the review proof code moves to `review_proof.rs`.
- A setup migration removes the installed `whiteboard/`, `whiteboard-canvas/`, `node/` and `hunk/` folders.

## Reason
Lighter; one display path.

## Consequences
- Amends 0383 (what is forked), 0394 (hunk review after every task: review notes replace it), 0607 (whiteboard's tools for the boss) and 0615 (whiteboard 0.5.2).
- Comments may cite records; no code references remain.
