# 0615 Whiteboard moves to v0.5.2 before the benchmarks, with animations on

- Status: v1
- Kind: product
- Phase/slice: program
- Date: 2026-11-14

## Question
Whiteboard is pinned at v0.2.4 (0564). 0608 asked for a spike of v0.5.2. Does Jocasta move to it, when, and with its animations?

## Facts
Verified 2026-11-14 by the spike (report `whiteboard-052-spike.md` in `D:/jocasta-spikes/sota/reports/`, outside the repo; built and run there, with screenshots).
- **Animations:** v0.2.4 has none; the vision (V6) names diagrams and animations. They arrived in 0.5.1 as experimental and off by default; 0.5.2 added their tour, clip blocks and `session_render`. One preference on whiteboard's server turns them on (`PUT /preferences/animations`).
- **The server side is compatible:** the host script's path, its environment names, the ready line, the discovery file, `/health`, `/control` and `/reviews-api` are as Jocasta uses them.
- **The canvas bridge is not.** Jocasta's unported page on the 0.5.2 canvas: Home throws and a review answers 404. The port is about 180 changed lines: the bridge and its mirrored contract (`src/frontier/canvas/host.ts`, `contract.ts`, `main.ts`), the build script, the pins and the tests. A ported copy type-checks and renders in the spike folder: Home, a board, the diff tab, an animation playing and its tour.
- **The animation stage** is a sandboxed frame that Jocasta's page rules block (no `.html` content type, `frame-ancestors 'none'`, same-origin resource policy). With an exception for the stage's files only, about 8 lines, it plays.
- **`session_render`:** an agent asks for a picture of one frame of its animation. Whiteboard's desktop app captures its own window. A browser page has no such capture: the spike answered with a placeholder.
- **The tutorial is gone upstream:** `POST /tutorial/open` answers 404 and the build's `build:tutorial-assets` step no longer exists, so Jocasta's build fails at that step and the "tour" button on the empty canvas has nothing to open.
- **Tools:** 23 become 25, or 27 with animations on. Their full descriptions grow from about 5.4k to 6.4k or 7.1k tokens, in the boss only (0607). Two tool names go (`session_restore`, `session_register_repository`); Jocasta names neither.
- **`session_create` with `open: true`** reports an error on today's path: Jocasta's answer to `openApiReview` lacks `softwareMapEnabled`. The port fixes it.
- **Sharing:** 0.5.1 added sharing a board as a link, which uploads it and needs a sign-in. The canvas shows whiteboard's "Share review" control; whether it showed under 0.2.4 was not checked.
- **Benchmarks:** no benchmark script touches the canvas. The boss rows carry the boss's tool list.
- **Not verified by the spike:** the shipped layout (`shipWhiteboardRuntime`, the runtime's new dependencies with native binaries), a built `jocasta.exe`, and Jocasta's own page route (the spike mirrored it).
- **The tag:** `v0.5.2` is commit `03d373ab0bf64ae3e3a5e4e1e592cb4fd173871d` on the submodule's remote.

## Options
| Option | For | Against |
|---|---|---|
| Now, animations on | V6 in daily use; one freeze, the benchmarks measure what ships | the canvas seam changes shortly before the VM runs; the tour button goes; no picture for `session_render` |
| Now, animations off | two tools fewer | the bump's main gain is unused |
| After the benchmarks | nothing moves in the canvas before the VM runs | no animations until then; the shipped build differs from the measured one, or the boss rows run again |
| Stay on v0.2.4 | nothing to do | no animations |

## Recommendation
Now, with animations on. Before the spike the recommendation was to wait until after the benchmarks; the spike showed the port to be small and known.

## Choice
- **Whiteboard is pinned at v0.5.2** (the submodule at the tag), before the benchmarks.
- **The canvas bridge and its contract are ported** as the spike's copy shows.
- **Animations are on:** Jocasta turns whiteboard's preference on itself; he sets nothing.
- **The animation stage gets a serving exception** limited to the stage's own files.
- **The tour button goes** with whiteboard's tutorial, and the build drops the tutorial step.
- **`session_render` gives no picture** from Jocasta's page: the agent is told so. He sees the animation; the agent cannot check it by image.

## Reason
His answer: `now-animations-on`.

## Consequences
- Amends 0564 (the pin) and 0393 (the canvas host page). 0383's seams and list are checked live on the new pin before the benchmarks.
- The boss's tool list grows by four tools.
- The shipped layout and a built `jocasta.exe` are verified by the build, not by the spike.
- Whiteboard's sharing is not decided here: nothing uploads without a sign-in, and he has not signed in.
