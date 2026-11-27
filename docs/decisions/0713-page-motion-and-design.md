# 0713 Page motion and design

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, M2 R7 (its iced form, after the amendment of 2026-11-27)
- Date: 2026-11-26 (rewritten for the native views 2026-11-27)
- Amends: 0678 (the pages' motion and look; the HTML pages themselves went with 0721), 0721 (the tokens, the walk, links on nodes, map rows), `DESIGN.md` (Motion, Do's and Don'ts)

## Question
The plan's R7 fixed the brief pages' motion and break-ui cases in CSS and JavaScript (a curve, two duration tokens, an entrance, a pulse, Play / Prev / Next, `dir="auto"`, wrapping, top-down flows). The amendment of 2026-11-27 replaced the pages with iced windows (0721), whose look follows ultron and whose process is Emil Kowalski's skills. What does R7 become in the windows?

## Facts
- iced 0.14 has `Animation` with a custom easing, a `canvas` with its own `update` (pointer events, a published message, a redraw), `mouse_area` (enter, exit, press), `time::every` and `window::frames` subscriptions, and no accessibility tree: nothing a window draws is announced to a screen reader.
- iced's text uses `Shaping::Auto` by default: ASCII is shaped basic, anything else advanced (cosmic-text: bidi, font fallback), and `Alignment::Default` aligns a right-to-left paragraph to the right. That is `dir="auto"`.
- `text::Wrapping::WordOrGlyph` breaks a word longer than the line anywhere (`overflow-wrap: anywhere`).
- iced has no widget scale, so R7's `:active { scale(0.97) }` over `--t-fast: 160ms` has nothing to drive (0721).
- Decision 15 (2026-11-26): all motion within 300 ms and on one curve, never replayed after a button press. R7's own timings were 180 ms (node entrance, disclosures) and 220 ms (edges, the Before / After switch); its active-node pulse ran twice for 600 ms each.
- The brief review found Play wrapping around forever, the caption re-announced every 700 ms, the highlight animating on Prev / Next, and a node's `detail` (a meeting URL, a review verdict) visible only through Play.

## Options
| Option | Pros | Cons |
|---|---|---|
| R7 in iced: one curve, two tokens (fast 180 ms, base 220 ms), the first-open entrance only, no pulse, Play stops at the last node, details and links on the nodes, map rows lit on hover, break-ui by iced's shaping and wrapping | Everything within 300 ms and on one curve; nothing moves after a press except what the press asked for | No press scale and no screen-reader caption (iced can't) |
| Keep R7's 160 ms token and the pulse | Closer to the CSS text | 160 ms has nothing to time; the pulse runs 1.2 s, past decision 15's 300 ms |

## Recommendation
R7 in iced, as the first option.

## Choice
His answers of 2026-11-26, the approved plan and its amendment of 2026-11-27:
- **One curve, two tokens.** `cubic-bezier(0.23, 1, 0.32, 1)` for every motion; `fast` 180 ms for what appears in place (a node's fade and 4 px rise, a disclosure: `✓ proven`, `Show code`, `Show its code`, a Brings row's proof) and `base` 220 ms for what travels or swaps (an edge drawing itself, the system's Before / After switch, 0710). `motion.rs` holds both (`FAST`, `BASE`); nothing else names a duration. R7's 160 ms press token has no twin: a press shows at once on the button's ground.
- **Entrance, first open only.** Nodes fade in and rise 4 px over `fast`, 25 ms later per layer up to the fifth; edges draw over `base` from 80 ms; everything has settled by 305 ms. The active node keeps a static ring: the pulse goes (two 600 ms beats break the 300 ms rule and hid the ring the review wanted seen).
- **No replay.** The entrance is a function of the time since the window opened; an action's answer patches its card or takes the brief without touching it; Play, Prev, Next and hovering move nothing.
- **Play / Prev / Next** under every flow with nodes (not under a system, which has its switch): Play walks the nodes layer by layer, each layer in its order, one every 700 ms, and stops at the last; Play from the last starts again at the first; Pause stops it; Prev and Next step by hand and stop Play. The node they are on carries a static ring in the text colour, and a caption under the flow says `<n> of <count> · <label> · <detail>`. iced has no accessibility tree, so nothing is announced; should iced gain one, only the first caption of a Play is announced.
- **A node's detail on the node**, under its label in the muted colour; when the detail is a web address it is drawn as a link (blue, the text colour under the pointer, without its `https://`), and a click opens it with the system opener.
- **Map rows**: a row under the pointer is lit (a faint blue ground, 6 px corners), and so is each row it links to; the `↔` line names them. Arcs between rows are not drawn: the rows are laid out by widgets, and lighting the linked rows says the same.
- **Reduced motion** (`JOCASTA_REDUCED_MOTION=1`; iced exposes no desktop setting): opacity only, no rise, no drawing, no growing.
- **Break-ui**: every title, heading, label, fact, check command, map label, list row, note, card and caption wraps anywhere (`WordOrGlyph`); paths in code wrap by glyph; text shapes and aligns by its own direction (`Shaping::Auto`, `Alignment::Default`); a flow with 4 or more layers or in a window under 720 px is drawn top down; map bars are never thinner than 2 px; an empty section says `Nothing here.`; a check or works row with no exit says `didn't run`, and no duration (or 0) says `not timed`; a closed thread's page says `This thread is closed; this page is its record.` instead of its buttons.
- **Look** (0721, unchanged): the palette file's colours and font, 6 px corners, hairlines, the type scale 12/13/15/18/26, outlined buttons with the primary one blue, the focus ring blue.

## Reason
Motion that says where something came from and stops; the rest still, so a press is never followed by something he didn't ask for. The break-ui cases are what a real review hands the window: long identifiers, Macedonian and right-to-left text, long chains, empty sections.

## Consequences
- `motion.rs`: `FAST` and `BASE` (the old `DISCLOSURE` and `SWITCH` go), `STEP` (Play's pace).
- `flow.rs`: `FlowCanvas` takes the step it rings and the message a link sends; it lights a link under the pointer and opens it on a click.
- `app.rs`: `Walk` and the flow's controls and caption, the map's lit rows (`Message::Hover`), wrapping on every text.
- `model.rs`: `steps`, `caption`, `node_link`.
- `DESIGN.md`: Motion and Do's and Don'ts say the above.
