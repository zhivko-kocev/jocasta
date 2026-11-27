# 0721 Native views with iced

- Status: v1 (built 2026-11-27)
- Kind: product, security
- Phase/slice: ultron 0.3 iteration, M2 amendment of 2026-11-27, V1
- Date: 2026-11-27
- Amends: 0678 (briefs are native windows; the loopback pages, their routes and hardening go) and 0698 (the palette file's colours and its new `font` reach the windows; the `:root` block goes)

## Question
The brief pages are HTML served on loopback and opened in his browser: a page engine in JavaScript and CSS, a cookie, launch codes, Host/Origin/CSP rules, and a browser tab that ultron has to route. Does Jocasta keep them, or draw its briefs itself?

## Facts
- iced 0.14 (MIT) draws native windows on Linux (Wayland, X11), Windows and macOS; it renders reactively (a window redraws only on an event or a requested frame), has `iced::Animation` with custom easing, a `canvas` widget and an `image` widget. Its `tiny-skia` renderer draws on the CPU, so no GPU driver is needed.
- On Linux a window's app id is set through `window::Settings::platform_specific.application_id`; sway and every Wayland compositor match on it.
- The service already answers `jocasta thread …` and `status --json` over its bridge pipe with the token file (0544, 0675); `brief_actions::act` runs every brief action and answers the whole brief.
- iced exposes no "reduce motion" setting of the desktop.

## Options
| Option | Pros | Cons |
|---|---|---|
| Native windows with iced, one process per brief | No server pages, no browser, no cookie or launch codes; the window has its own app id ultron can place; the look follows ultron | A GUI dependency; a second drawing of every section |
| Keep the HTML pages | Built | A browser tab per brief, a hardened web server, JavaScript to maintain |
| A webview | HTML reused | A browser engine per window, heavier than iced |

## Recommendation
Native windows with iced.

## Choice
His answers of 2026-11-27 (plan amendment "native views"):
- `jocasta view <brief-id>` opens one native window for the stored brief `<data>/briefs/<id>.json`, titled with the brief's title. Linux app id by kind: `jocasta-review` (`thread-*`, `pr-*`), `jocasta-brief` (the morning and weekly briefs), `jocasta-show` (the boss's `show`). An id that isn't stored: `jocasta: no brief <id> is stored.`, exit 1.
- Wherever Jocasta opened a brief's URL (`show`, the Done notification's link, the dock's `b` and Enter on a BRIEFS row) it now starts `<its own program> view <id>`, detached, on every OS. A brief's link in `today` and in a Done toast is `jocasta:brief/<id>`; it is no web link and opens only that way.
- The view reads the brief file; only the service writes it. Its buttons go to the running service over the bridge pipe as a CLI peer: `brief act {id, action}` runs `brief_actions::act` (the same Allow gate) and answers the brief as JSON (a `polish`/`reword` suggestion included); `brief live {id}` answers whether a thread brief's thread is open. The view patches the affected card (or the thread's buttons) from the answer; unsaved text in the other cards and the focus stay.
- A thread brief whose thread is closed says `This thread is closed; this page is its record.` instead of buttons. Without a running Jocasta a button answers `Jocasta isn't running.`
- Sections drawn: text, facts, checks, works (passed / failed / not run, duration or `not timed`, output tail, screenshots shown from `<data>/briefs/assets/<id>/`, opened in the system image viewer on click), flow (a canvas: nodes by layer and order from `layout.rs`, curved edges with labels, the active node ringed, the failed one red; top-down when it has 4 or more layers or the window is under 720 px wide), map (groups of files with their bars), diff (monospace, `+`/`−` coloured, closed behind `Show code (<n> files, +a −b)`), cards (severity, kind, place, verdict switch, `✓ proven` disclosure with command, expectation, exit and tail, his edit, Publish), list. An empty section says `Nothing here.`
- Look: the palette file's colours (0698), and its new optional `"font"` (a family name ultron writes) for every text; without it the system's sans for prose and its monospace for code. Corners 6 px, hairlines, type scale 12/13/15/18/26, the window's own background.
- Motion (R7's tokens): one curve `cubic-bezier(0.23, 1, 0.32, 1)`, `base 220ms` for edges and 180 ms reveals. Only the first open moves: a flow's nodes fade in and rise 4 px over 180 ms, 25 ms later per layer up to 5, its edges draw over 220 ms from 80 ms (settled by 305 ms); disclosures fade open over 180 ms (`iced::Animation`). A press shows at once (a hover and press ground; iced has no widget scale, so R7's `fast 160ms` press scale has no twin). Nothing replays after a button press. `JOCASTA_REDUCED_MOTION=1` keeps opacity only (no rise, no drawing).
- Renderer: iced's `tiny-skia` only (no wgpu), default features off.
- Deleted: `src/frontier/brief/routes.rs`, `page/brief.{html,js,css}`, the launch codes (`?t=`), the page token and its cookie, the brief routes' Host and Origin checks, the content-security-policy, `/brief/assets/…`, `tui::palette_css`. The API's own Host check and bearer token stay.

## Reason
A brief is something he reads and acts on beside his work; a native window with its own app id needs no browser, no web server surface and no JavaScript, and looks like his desktop.

## Consequences
- New `src/frontier/view/` (the window, its view model and its motion), `Route::View`, the bridge commands `brief act` and `brief live`, `link::open_file`.
- `BriefLinks` keeps `url` (now `jocasta:brief/<id>`) and `open` (spawns the view); `ServerOptions.briefs` stays the test seam.
- Plan appendix I: the brief routes, launch codes and page cookie are gone; `jocasta view` and the two bridge commands are new; the palette file gains `font`.
- `jocasta brief render` (R8) is not built; `jocasta brief open <spec.json|->` stays R8's.
- Dependencies: iced 0.14 (`tiny-skia`, `wayland`, `x11`, `canvas`, `image-without-codecs`, `tokio`, `crisp`) and `image` 0.25 with only the PNG, JPEG, GIF and WebP codecs (iced's `image` feature would bring every codec, an AV1 encoder and an NCSA-licensed fuzzer crate among them); `cargo-machete` ignores `image`, used only through iced. `deny.toml` gains one advisory ignore, RUSTSEC-2026-0192 (`ttf-parser` unmaintained, iced's font reading); no licence was added. Dev-dependency `iced_test` 0.14 draws the windows offscreen for the ignored `view_shots` test.
