# 0369 Frontier looks like JARVIS, composed

- Status: decided
- Kind: product
- Phase/slice: 4/4.3, 7.3
- Vision: V1, V6
- Date: 2026-10-09

## Question
0258 made Frontier a plain minimal dark UI. He asked for a JARVIS look; the first JARVIS prototype read as too aggressive. How does Frontier look?

## Facts
Checked 2026-10-09.
- 0258: a clean minimal dark UI (one blue accent on near-black), motion only for state changes.
- On 2026-10-09 he asked, through the impeccable design skill, for two themes: "JARVIS" (midnight, navy, electric cyan, precision type, no generic dashboard, no excess glow) and "Vintage Intelligence" (walnut, brass, green, ivory). A prototype outside the repo showed both on Frontier's layout. His verdict: only JARVIS, and softer.
- The references he named: devdotfast/whiteboard (its canvas animations), herdr (its tabs), omp.sh and opencode. Read the same day: herdr removed every ambient animation in 0.8.0 ("motion should mean something changed"); opencode and omp keep tool calls to one quiet line and dock questions above the input.
- The app window's Content-Security-Policy allows only Frontier's own origin (0368), so fonts ship with Jarvis.
- Mona Sans (GitHub, variable width and weight) and Geist Mono (Vercel) are under the SIL Open Font License 1.1, which allows bundling with the licence alongside.

## Options
| Option | Pros | Cons |
|---|---|---|
| JARVIS, composed: midnight ink, cyan used only as light, champagne for what awaits him, bundled faces, motion only on change | The film's calm; readable all day | Two font files in the repo |
| The first JARVIS prototype: saturated cyan fills, tracked capitals, corner ticks | The most "HUD" | He found it too aggressive |
| Keep 0258 | No work | Not the JARVIS he asked for |

Fonts (asked on 2026-10-09): bundled OFL faces | system fonts only.

## Recommendation
JARVIS, composed, with bundled OFL fonts.

## Choice
- One look, no theme setting. Midnight ink (#06090f to #131c29) with blue-tinted hairlines. Cyan (#92d6ec) is light, never paint: lines, small marks, short text, washes under 10%. Champagne (#d9c08f) marks what awaits him, rose (#e39a9a) a failure, sage (#9cc9b0) what landed.
- Sentence case everywhere; the JARVIS wordmark is the only tracked capitals.
- Type: Mona Sans for the interface (wide and light for the greeting and the wordmark) and Geist Mono for code and figures, both bundled as woff2 with their licences. Thread terminals keep Cascadia Mono for its full glyph coverage, in the look's ANSI palette; code on the canvas uses a shiki theme drawn from the same colours.
- The Presence is Jarvis's mark: two hairline rings and a core, in the HUD, the greeting and the input. It is still when idle; it echoes while he holds push-to-talk, a short arc circles while the boss works, and the inner ring breathes while Jarvis speaks.
- While Jarvis speaks, the sentence being spoken brightens and gains an underline in the boss chat.
- Motion stays event-driven (0258): what changes resolves into place once; nothing moves while idle.

## Reason
"okay only JARVIS but i think this needs more iterations it is to attacky i want smooth like jarvis is to tony the system uses british accent it shouldnt be so bold, i need next level design use impeccable". Fonts: "Bundle OFL fonts". Reach: "Everything" (chrome, cards, toasts, today, the canvas, the terminal palette and the code theme).

## Consequences
- Amends 0258: the look; its motion rule stands.
- The Vintage Intelligence theme is dropped.
- `src/frontier/ui/fonts/` holds the two faces and their OFL texts; README's credits name them.
- The exact values above were chosen by the builders under 0363 and are listed as built.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0395: the look is re-decided in the UX pass before F2.
- Amended by 0401: the companion's colours are Tokyo Night, shared with herdr and omp.
- Amended by 0415: state colours follow herdr's Tokyo Night meanings.
- Amended by 0432: quiet text and the focus ring meet 4.5:1.
- Amended by 0548: herdr becomes the whole work surface; the rail moves into a `jocasta` text pane and the companion window becomes the canvas; the look of what remains is decided in the herdr experience pass.
- Amended by 0555: the Presence orb and the greeting go.
