---
name: Jocasta companion
description: "JARVIS, composed, on Tokyo Night: a rail where Jocasta greets him and keeps today, whiteboard's canvas, and on his second screen a third column with the news, weather, activity and machine."
colors:
  bg-0: "#1a1b26"
  bg-1: "#232636"
  bg-2: "#24283b"
  bg-3: "#2d3650"
  line-1: "rgb(86 95 137 / 22%)"
  line-2: "rgb(86 95 137 / 40%)"
  line-3: "rgb(86 95 137 / 65%)"
  fg: "#c0caf5"
  fg-2: "#a9b1d6"
  fg-3: "#8a93bf"
  mark-3: "#697196"
  blue: "#7aa2f7"
  cyan: "#7dcfff"
  blue-wash: "rgb(122 162 247 / 10%)"
  blue-line: "rgb(122 162 247 / 40%)"
  yellow: "#e0af68"
  red: "#f7768e"
  red-line: "rgb(247 118 142 / 38%)"
  green: "#9ece6a"
  code-comment: "#565f89"
  code-keyword: "#bb9af7"
  code-number: "#ff9e64"
typography:
  display:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "26px"
    fontWeight: 300
    lineHeight: 1.2
    letterSpacing: "-0.005em"
    fontVariation: "'wdth' 112.5"
  title:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 500
  body:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.55
  summary:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14.5px"
    fontWeight: 400
    lineHeight: 1.5
  row:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "13.5px"
    fontWeight: 400
  meta:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
  label:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 400
  code:
    fontFamily: "Geist Mono, Cascadia Mono, Consolas, ui-monospace, monospace"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.65
  mono-data:
    fontFamily: "Geist Mono, Cascadia Mono, Consolas, ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
  mono-key:
    fontFamily: "Geist Mono, Cascadia Mono, Consolas, ui-monospace, monospace"
    fontSize: "10.5px"
    fontWeight: 500
rounded:
  key: "4px"
  s: "6px"
  m: "10px"
  l: "14px"
  pill: "999px"
spacing:
  "2xs": "4px"
  xs: "6px"
  sm: "8px"
  md: "10px"
  lg: "14px"
  xl: "18px"
  "2xl": "22px"
  "3xl": "28px"
components:
  button:
    backgroundColor: "transparent"
    textColor: "{colors.fg}"
    rounded: "{rounded.s}"
    padding: "6px 12px"
    typography: "{typography.body}"
  button-hover:
    backgroundColor: "rgb(122 162 247 / 5%)"
  button-lit:
    backgroundColor: "{colors.blue-wash}"
    textColor: "{colors.cyan}"
    rounded: "{rounded.s}"
    padding: "6px 12px"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.fg-2}"
    rounded: "{rounded.s}"
    padding: "6px 12px"
  button-small:
    typography: "{typography.label}"
    padding: "3px 9px"
    rounded: "{rounded.s}"
  tool-button:
    textColor: "{colors.fg-2}"
    rounded: "{rounded.s}"
    height: "30px"
    padding: "0 6px"
  rail:
    backgroundColor: "{colors.bg-0}"
    width: "clamp(300px, 16vw, 360px)"
    padding: "28px 18px 14px 24px"
  extras:
    backgroundColor: "{colors.bg-0}"
    width: "clamp(360px, 22vw, 440px)"
    padding: "28px 24px 14px 22px"
  today-row:
    textColor: "{colors.fg}"
    typography: "{typography.row}"
    height: "26px (a second 12.5px line in fg-3 when the row has one)"
  card-ask:
    backgroundColor: "{colors.bg-2}"
    textColor: "{colors.fg-2}"
    typography: "{typography.meta}"
    rounded: "{rounded.l}"
    padding: "10px 12px"
  toast:
    backgroundColor: "{colors.bg-1}"
    textColor: "{colors.fg}"
    typography: "{typography.meta}"
    rounded: "{rounded.m}"
    padding: "8px 12px"
  toast-hover:
    backgroundColor: "{colors.bg-3}"
  presence-rail:
    textColor: "{colors.blue}"
    size: "56px"
  presence-capsule:
    textColor: "{colors.blue}"
    size: "22px"
  capsule:
    backgroundColor: "{colors.bg-2}"
    textColor: "{colors.fg}"
    typography: "{typography.meta}"
    rounded: "{rounded.pill}"
    padding: "7px 16px 7px 10px"
  state-mark:
    rounded: "{rounded.pill}"
    size: "7px"
  canvas-side:
    backgroundColor: "{colors.bg-1}"
  canvas-bar:
    backgroundColor: "{colors.bg-0}"
    textColor: "{colors.fg}"
    typography: "{typography.label}"
  source-panel:
    backgroundColor: "{colors.bg-0}"
    rounded: "{rounded.m}"
  kbd:
    textColor: "{colors.fg-2}"
    typography: "{typography.mono-key}"
    rounded: "{rounded.key}"
    padding: "0 5px"
---

# Design System: Jocasta companion

## Overview

**Creative North Star: "JARVIS, composed", on Tokyo Night**

The companion is one window beside herdr (0392, 0409). The work happens in herdr's panes; this window greets him, keeps today in view, speaks, listens, and shows whiteboard's canvas, where agents draw (0393). Jocasta holds a rail on the left (300px, up to 360px on large screens): his Presence, a greeting by the hour, one sentence on who is at work and how much awaits him, today's Calendar / Awaiting you / Landed / Underway / Next (0416, 0432), the day's spend, and at its foot how to talk to him. The canvas takes the middle. With a second display the window is the dashboard on it (0428): a third column on the right carries the clock and weather, the news, today's activity and the machine with Jocasta's services (0429). There are no thread tabs, terminals, boss chat, docked request cards, crew pills or projects sheet: those live in herdr.

The colours are herdr's and omp's Tokyo Night (0401), so the companion and the terminal read as one product. The layout, fonts and motion are 0369's: calm, typographic, light that falls on things rather than paint. Density is moderate. Surfaces are separated by hairlines and close steps of the Tokyo Night ground, not by contrasting panels. Only what asks something of him (the download questions) carries a lift.

While the window is hidden in the tray, holding Right Ctrl shows the **capsule**: a small pill window that says Listening…, Writing it down…, then Sent to the boss (0409); a failure reads "Dictation: <why>" in red on one line, cut with an ellipsis, the whole line as its tooltip.

**Key Characteristics:**
- Tokyo Night ground (#1a1b26 for the rail, #232636 for the canvas side) with hairlines drawn from its overlay colour at 22%, 40% and 65%.
- Blue (#7aa2f7) only as light: the Presence, marks, lines, short text, washes at 10%; cyan (#7dcfff) for text that takes the light.
- State colours mean what herdr's Tokyo Night means (0415): working yellow, blocked (awaiting him) red, done teal, idle green; nothing else.
- Mona Sans wide and light for the greeting; regular for everything else. Geist Mono for code, paths, keys and cost.
- Sentence case everywhere; no uppercase or tracked labels.
- Motion is event-driven: things resolve into place once; nothing moves while idle.

### Motion

The vocabulary lives on `:root`: an out-expo ease (`cubic-bezier(0.22, 1, 0.36, 1)`) for arrivals, an in-out ease (`cubic-bezier(0.65, 0, 0.35, 1)`) for strokes and breath, and three durations (quick 180ms for hover colour, calm 420ms for state marks, rows and cards, slow 640ms for the summary and toasts).

**The Still-When-Idle Rule.** Nothing animates unless something changed (0258). Arrivals use one `resolve` motion (fade, 4px rise, 4px blur clearing) and stop: the greeting's summary when its words change, today's rows, toasts, the download question, the capsule's words. Loops exist only while a live state holds: the Presence's echo (1.4s) while he holds push-to-talk, its orbit (1.8s) while the boss pane works (herdr reports it `working`), and its breath (1.1s) while Jocasta speaks. The spend rolls to a new value once (600ms) and rests.

**The Settle-Not-Slide Rule.** Nothing slides in from an edge. The canvas fades in place (220ms) when it mounts.

**The Reduced Motion Rule.** Under `prefers-reduced-motion: reduce`, every animation and transition of Jocasta's own parts ends at once (1ms, one iteration) and figures jump to their value. No motion of Jocasta's is exempt; whiteboard's canvas keeps its own reduced-motion design.

## Colors

herdr 0.9.5's Tokyo Night (`src/app/state.rs`, `tokyo_night`), mapped onto the companion's roles. `src/frontier/ui/palette.css` is the one palette file (0474): its `:root` custom properties carry the names below, and it is the only file with Tokyo Night's hex values; translucent colours are built from them (`rgb(from var(--blue) r g b / 10%)`). `styles.css` imports it, the page's HTML links it first (so the first paint is bg-0), `whiteboard.css` uses its variables, and `palette.ts` reads it for what CSS can't colour: Monaco's theme and the Windows Terminal scheme `jocasta setup` writes. The front matter above mirrors it.

### Primary
- **Blue** (blue, herdr's accent): the light of the system. The Presence, the meeting and event marks, links, the caret, the lit range in the source panel, the voice button when on.
- **Cyan** (cyan, herdr's teal): text that takes the light: a lit button's label, a hovered row. Also the done state mark (0415).
- **Blue Wash** (blue-wash, 10%): the only blue surface: behind the lit button and the source panel's lit range.
- **Blue Line** (blue-line, 40%): blue borders: the lit button, the listening capsule. Focus outlines are full blue (0432).

### Secondary
- **Yellow** (yellow): working (0415), as herdr paints a working agent. The Underway row mark. Also what wants a look soon (0456): an Awaiting-you reviewed row's "waiting 3 h", an "overlaps …" line, a quota window at 80% or more (the shown, rounded percent), and a leftover worktree with unpushed commits or changes.

### Tertiary
- **Red** (red) and **Red Line** (red-line, 38%): blocked (awaiting him, 0415), failure and urgency. The Needs-you row mark, blocked, failed and urgent toasts, the rail's bad status, a dictation error in the capsule, the voice button's error mark.
- **Green** (green): idle (0415), as herdr paints an idle agent.

### Neutral
- **BG 0** (bg-0, herdr's panel): the window's ground, the rail, the canvas bar, the source panel, Monaco's ground (#1a1b26).
- **BG 1** (bg-1, herdr's active row): the canvas side, toasts.
- **BG 2** (bg-2, herdr's surface0): raised things: the download question, the capsule, the canvas's menus.
- **BG 3** (bg-3, herdr's selection): toast hover.
- **Line 1 / 2 / 3** (line-1, line-2, line-3): every division, from herdr's overlay0 (#565f89). Line 1 separates regions (the rail's and the third column's edges, today's and each section's top rule, the canvas bar); line 2 outlines toasts, cards, the capsule, kbd and the source panel; line 3 is a button's resting border (so it reads against the ground, 0432) and a toast's life line.
- **FG / FG 2 / FG 3** (fg, fg-2, fg-3): primary text (herdr's text), secondary text (subtext0 #a9b1d6: the summary, spend words, ghost buttons, kbd), quiet text (dates, row labels, projects, hints, second lines). FG 3 is #8a93bf, lifted from herdr's overlay1 so it reaches 4.5:1: 5.7:1 on bg-0, 5.0:1 on bg-1, 4.9:1 on bg-2 (0432).
- **Mark 3** (mark-3, herdr's overlay1 #697196): the queued ring (also a clean leftover worktree's ring, 0456); marks need 3:1, not 4.5:1.

### Code
Monaco's theme on the canvas (`palette.ts` reads `palette.css` and `whiteboard/monaco.ts` builds the theme from it; 0411, 0474): text fg, keywords mauve #bb9af7, strings green, numbers peach #ff9e64, types cyan, comments #565f89, errors red. Code colours carry syntax only; the One Meaning rule governs the interface, not code.

### Named Rules
**The Light-Not-Paint Rule.** Blue is light, never paint. It appears as lines, marks of a few pixels, short text, and washes at 10%; the 7px meeting mark and the source panel's lit-range rule are the norm. Text selection (24%) is the one wash above 10%, kept for legibility.

**The One Meaning Rule.** State colours mean what they mean in herdr's sidebar (0415): yellow working, red blocked (awaiting him) or failed, teal done, green idle. None of them is used for decoration or variety; a colour that appears means its state is true. Yellow's one other meaning is "wants a look soon" (a reviewed thread's wait, an overlap, quota past 80%, an unpushed leftover worktree; 0456), always as words or a mark beside what it qualifies.

**The Hairline Rule.** Regions are separated by hairlines and adjacent ground steps, never by contrasting panel fills or heavy rules.

## Typography

**Display and Body Font:** Mona Sans (variable weight 200 to 900, width 75% to 125%), with Segoe UI Variable Text, Segoe UI, system-ui.
**Mono Font:** Geist Mono (variable), with Cascadia Mono, Consolas, ui-monospace.

Both faces are bundled as woff2 under the OFL and load from the page's own origin (strict CSP, offline).

### Hierarchy
- **Display** (300, 26px, 1.2, width 112.5%, -0.005em): the greeting ("Good afternoon.") only.
- **Title** (500, 15px): Today's heading.
- **Summary** (14.5px, 1.5): the greeting's second line, in fg-2.
- **Body** (400, 14px, 1.55): the default.
- **Row** (13.5px): today's rows.
- **Meta** (13px): the date, the spend's and the quota's words, toasts, the download question, the capsule.
- **Label** (12.5px): row labels (Calendar, Awaiting you, Landed, Underway, Next; Tech, Gaming, Events, Worklog, Saved to memory), projects beside a row, a row's second line (an "overlaps …" line, a leftover's branch line), a reviewed row's wait, the hint, the status, small buttons, the canvas bar.
- **Clock** (Mona Sans 300, 32px, width 112.5%, tabular figures): the dashboard column's time; the one display-size figure besides the greeting.
- **Code** (Geist Mono 400, 12.5px, line 19px): Monaco's peeks, Diff tab and source panel. **Mono Data** (12px) for a meeting's, reminder's or event's time, the spend figure (500, 12.5px), the machine's figures, paths in the source panel's head; **Mono Key** (500, 10.5px) for kbd.

### Named Rules
**The No Capitals Rule.** Sentence case everywhere. No labels, kickers or headings are uppercased or letter-spaced.

**The Tabular Figures Rule.** Every figure that updates in place (the spend, a reminder's time) is set with tabular numerals so it never jitters.

**The Mono-Means-Literal Rule.** Geist Mono is used only for text that is literally code, a path, a key, a cost, an id or a time; interface prose is always Mona Sans.

## Layout

The laptop window is a two-column grid: the **rail** (`clamp(300px, 16vw, 360px)`) and the **canvas side** (the rest). The dashboard window on his second screen (0428), from 1200 CSS px wide, gains a third column, the **extras** (`clamp(360px, 22vw, 440px)`; 1920px gives about 307 + 1191 + 422); the laptop window never has it, whatever its width (0439). No HUD strip: herdr's tab bar carries the figures (0403), `N at work · N awaiting you · $X today · 5h N% · 7d N%`, the quota suffix left off while omp has reported none (0456). While the page isn't connected, text keeps its full contrast; only the state marks dim to 55% and the status line says "Reconnecting…" (0440).

**The rail** is bg-0 with a line-1 right edge, padded 28px top, 24px left, 18px right, 14px bottom, as a column:
1. **The greeting:** the Presence (56px; a toggle button that starts and stops dictation, `aria-pressed`), the display line, the summary ("Two at work. Four would like a word.", counting every Awaiting-you row, the HUD's number; "Nothing awaits you." at zero, "All quiet." when nothing is at work; 0432), and, only when it isn't live, a status line ("Reconnecting…", "Stopped", the port notice) in fg-2, red when broken.
2. **Today** (scrolls when long), under a line-1 rule with 18px above its heading: "Today" in title type with the date (`Tue 13 Oct`) in fg-3; then up to five groups, each a label-size fg-3 label over its rows: **Calendar** (blue mark; today's meetings from the iCal poller, the time in a fixed 44px column, mono, or "all day" in sans; a meeting with a link opens it with a ↗; meetings begun more than 15 minutes ago in fg-3; one begun within 15 minutes says "now" and the next one says "in 21 min", both in cyan; 0416, 0429), **Awaiting you** (red mark; one line `<subject>`, the project in fg-3, and its detail as a second line, e.g. "Retry on 429 · acme-api / plan ready" or "Review requested ↗ / Add rate limits (acme/api #42)"; a row with a pane focuses that pane in herdr, one with a link opens it, a reminder shows its time; 0432; and a reviewed row for each thread that is reviewed and not finished, oldest first, kept however long it waits: the done mark, `<title>` over its thread line, and "waiting 3 min", "waiting 3 h" or "waiting 2 d" in yellow beside the project; 0456), **Landed** (teal mark; recently finished work, as 0263's digest means it: threads whose task ended and isn't reviewed yet, then threads merged or opened as a PR since yesterday, those without a pane to focus; the second line is the thread's line, e.g. "merged into main as 6796653" or "PR open"), **Underway** (yellow mark; the second line is the thread's last line), **Next** (a queued ring; inbox notes). A thread row in Awaiting you, Landed or Underway that touches the same files as another open thread adds a line "overlaps <title>, <title>" in yellow (0456). A group with nothing in it is left out; with nothing at all, "Nothing on the list. Hold Right Ctrl and tell Jocasta what to start." Under the groups, kept at Today's bottom edge while its rows scroll, the spend: `$4.82 today`, the figure in mono, or "Nothing spent yet."; under it the subscription quota `5h 42% · 7d 18%` (mono figures; "Subscription quota: " for screen readers; each window's reset time as its tooltip; a window at or past 80% in yellow, judged on the shown, rounded percent, as is its toast; hidden until omp reports it; 0456). Before the first snapshot the status line says "Connecting…" and Today shows no empty notices.
3. **The foot** (toast rows scroll in their own area and never draw over Today; the download question and the hint row stay in view, and the foot as a whole scrolls only as a last resort, the hint row staying at its bottom): toast rows, then a download question or a failed download when one waits, then the hint row: "Hold Right Ctrl to talk" (or dictation's progress: "Listening · release to send", "Listening · click again to send" when the Presence started it, "Writing down what you said…", "Getting the speech model…" with the percentage beside it as a progress bar, "Dictation: …"; a polite live region) and, at the right, the voice button (named "Voice", pressed when on; its state "Voice on", "Voice off", "Voice: not downloaded", "Voice loading 40%", or a red error mark with "Voice: not working" as its title and description).

**The extras** (the dashboard's third column, 0429) are bg-0 with a line-1 left edge, padded 28px 24px 14px 22px, as a column with 18px between its parts; each section under a line-1 rule, a title-type heading and the rail's row look, nothing on a card:
1. **The head:** the clock (`14:20`, minute steps) and, at the right, the weather when `dashboard.weather` is set: the temperature in 24px light wide sans, the summary ("Light rain") in fg-2 over the place and the high/low in fg-3. Unset, it isn't there; set but failing before a first answer, "Weather unavailable: <why>".
2. **News** (takes the height left, scrolls): one label per feed (Hacker News, LWN, Gaming; 0442), each headline one line in fg turning cyan on hover with its source and age ("Hacker News · 31 min") as a second line. Headlines open in the system browser. A failed refresh says "The news didn't refresh; this is the last list." with the reason in fg-3.
3. **Today's activity** (up to 36% of the column, scrolls): Events (the latest `[event]` lines with their time), Worklog (one line per project), Saved to memory (today's compaction and handoff summaries with time and project); "Nothing yet today." when empty.
4. **Machine & services** (refreshed every 10 s, 0443): CPU, Memory ("11.4 GB of 32.0 GB") and Disk ("212 GB free of 953 GB") with mono figures right-aligned, then Graphiti, whiteboard and herdr: "up" in fg-2; "starting" or Graphiti's "idle · starts when needed" with a queued ring and the words in fg-3, never red (0444); or a red mark and "down · <why>" in red ("down · Docker isn't running").
5. **Left on disk** (only while there is something in it; 0456): the note "Closed threads' worktrees, kept until you say to remove them." in fg-3, then one row per worktree in `<project>.wt/` that no open thread holds: the folder with its project in fg-3, and a second line `branch · N ahead · N uncommitted` or `branch · N ahead · clean` ("no branch" when there is none); a yellow mark when it has unpushed commits or changes, else the empty ring. Refreshed at start and every 10 minutes; nothing here removes anything (the boss's `worktree_remove` does, on his word).

**The canvas side** is bg-1 and holds whiteboard v0.2.4's own canvas (its look, dark theme), mounted by Jocasta (0393, 0409). While a review is open, a slim bar on bg-0 with a line-1 hairline under it sits above it: a "Reviews" button (back to whiteboard's Home list of reviews) and the review's title in fg; on Home and in the empty state there is no bar (0432). The boss's `show(review)` opens that review. Code peeks, the Diff tab and "open the code" are Monaco 0.57.0 on the Tokyo Night theme (read-only, no minimap; 0411); opening a step's code shows the source panel over the canvas (right, about 70% wide) with a head (the path, "lines a–b · after the change" or "· before the change", Close) and the range lit by a blue-wash band with a 2px blue left rule; the panel takes focus, Escape anywhere closes it and focus returns to what opened it; a file that can't be read says so inside the panel. When whiteboard's server or build is missing, the side shows, centred like the empty state, "The canvas couldn't start." in title type over a plain reason and what to do ("Jocasta's whiteboard files are missing. Reinstall Jocasta; in a development checkout, run bun run build."), a "Try again" button only when trying again can help (the server didn't start in time or exited), and the raw error folded under Details (0432). Until the first review exists (the scratchpad doesn't count), the side shows Jocasta's empty state instead of whiteboard's Welcome (0419): centred, "Nothing on the canvas yet." in title type over "Ask Jocasta or a thread to draw a diagram or a review; it appears here." in summary type and fg-2, then a small default "Take the whiteboard tour" button that opens whiteboard's tutorial review (0432); it resolves in once. There is no draw-in, tour or scratchpad picker of Jocasta's own: whiteboard has its own.

**The capsule** is its own page (`/?view=capsule`) in a small frameless window: a transparent ground with one centred pill.

Spacing has no custom properties; the reused steps are 4, 6, 8, 10, 14, 18, 22 and 28px.

## Elevation & Depth

Flat by default, with tonal layering through the four ground steps: the rail (bg-0), the canvas side and toasts (bg-1), raised things (bg-2), hover (bg-3).

### Shadow Vocabulary
- **Lift** (`box-shadow: 0 1px 0 rgb(255 255 255 / 3%) inset, 0 10px 30px -14px rgb(0 0 0 / 75%), 0 2px 6px -2px rgb(0 0 0 / 50%)`): the download question, the canvas's source panel, and Jocasta's own windows' cards (0519).

### Named Rules
**The Raised-Means-Asking Rule.** Only a question waiting on him, or the source panel he opened over the canvas, carries a lift. Toasts, rows, panels and the capsule sit flat; a toast in its own window outside the companion carries the lift too, to read off whatever app is behind it (0519).

**The Glow-Once Rule.** Glow is a moment, never a resting state.

## Shapes

Soft, small corners and hairline strokes. Controls take 6px, toasts and the source panel 10px, the question card 14px; the capsule and state marks are fully round. Every outline is 1px. Icons sit on one 16-unit grid with a 1.4px round-joined stroke, drawn in-house as SVG (voice, voice off, close).

## Components

### Buttons
Quiet by default; the one that answers is lit, never filled.
- **Default:** transparent with a line-3 border and fg text, 6px 12px, 6px corners. Hover brings the border to fg-3 over a 5% blue tint.
- **Lit:** the answer: blue-wash behind, blue-line border, cyan text; hover brightens the border to full blue.
- **Ghost:** no border, fg-2, brightening to fg on hover.
- **Small:** 3px 9px at 12.5px (the question's answers, Try again on the canvas).
- **Tool buttons:** 30px square icon buttons (the voice button); fg-2, fg on hover, blue when on.
- **Focus:** a 1px full blue outline offset 2px (5.9:1 on bg-1), everywhere in Jocasta's own parts; in Today's and the column's rows the outline is drawn around the whole row, so the list's overflow never clips it (0432). Disabled is 40% opacity.

### State Marks
One drawn vocabulary of 7px circles, coloured as herdr colours agents (0415): working a yellow dot, blocked (awaiting him) a red dot, done a teal dot, idle a green dot; a meeting or plain event a blue dot, failed a red dot (a service down, a failed download), queued a mark-3 ring. Colour changes over 420ms.

### Today Rows
Rows of at least 26px: the state mark, then one line cut with an ellipsis at 13.5px and, when the row has one, a second line at 12.5px in fg-3 (a thread's question or last line, an item's detail, a headline's source and age). Every row that acts looks the same: words in fg turning cyan on hover; a link carries a small fg-3 ↗ and opens in the system browser (the app takes the navigation; nothing opens a window of its own). A project follows the first line in fg-3 at 12.5px.

### Question Card
The voice download ("Jocasta would like to speak. Her voice model, about 350 MB, …"; the voice is kokoro's female `af_heart`, 0418, 0480) and the dictation model download: bg-2, line-2 border, 14px corners, the Lift, 10px 12px; the words at 13px in fg-2 over two small buttons, Download (lit) and Not now (ghost).

### Failed Line
A failed voice download (0432): flat, bg-1 with a red-line border and 10px corners at the rail's foot; a red mark and the plain reason ("Voice download failed: there's no connection to the download server."), then a small Try again and a folded Details with the raw message in mono.

### Toast Rows
In-app toasts are rows at the rail's foot, never a layer over the canvas; while the window is hidden or unfocused the app shows the urgent and blocked ones in its own toast windows (0519), so the page has no notifications of its own (0407). A 7px state column and two clamped lines at 13px, 8px 12px, 10px corners, bg-1 with a line-2 border (red-line for blocked, failed and urgent), bg-3 on hover, and a 30px × at the right that closes it without focusing anything; Escape closes the newest (0432). A 1px line-3 life line along the bottom shrinks to nothing over 6s, paused while the toast is hovered or focused; blocked toasts stay until the thread no longer awaits him, urgent ones for 15 minutes or until handled, and both are kept across a reconnect or reload. Urgent ones sit in an assertive live region, the rest in a polite one. A click on the words focuses the toast's pane in herdr, or opens its link; while disconnected a pane toast stays put.

### The Presence (signature)
Jocasta's mark: an outer hairline halo (32% opacity), an inner ring (55%), and a 2.6-unit core (85%), all in blue, on a 36-unit viewBox with non-scaling strokes. Sizes: 56px at the top of the rail, where it is the dictation control (cyan on hover), and 22px in the capsule. Still while idle; while listening the halo brightens to 70% and an echo ring expands outward; while the boss pane works a short 1.4px arc orbits; while speaking the inner ring breathes.

### The Capsule
A pill (7px 16px 7px 10px) on bg-2 with a line-2 border, the 22px Presence and one 13px line that resolves in on each change: "Listening…" (blue-line border, the Presence echoing), "Writing it down…" (the Presence orbiting), "Sent to the boss" (the words as the tooltip), or "Dictation: <why>" in red with a red-line border. It only shows; the hidden window's page records and sends.

### Jocasta's Own Windows
Two questions and the toasts outside the window are Jocasta's own windows (0519): frameless, sandboxed and on top, each showing the page's popup view on a transparent ground, so they show even while the window is hidden. The close question (0434, 0438): "Stop N working thread(s)? Their sessions are saved." with Stop everything and Keep running in background, Keep the default. The confirmation (0484), titled "Jocasta: Allow or Deny": one sentence ("Merge <p>/<t> into <trunk>?", "Open a PR for <p>/<t>?", "Remove the leftover worktree <name> of <p>?", "Start the thread "<t>" in <p>?", "Save this lesson for <p>? Every new thread there reads it first.") over a detail text (the commit count and warnings, the brief, the lesson in full with its source, "Source: none given; it's saved as your word" when the boss gave none), with Allow and Deny; Deny is the default, and after 2 minutes unanswered the window closes as Deny. Both are the question card (bg-2, a line-2 outline, the lift), 480px wide, centred on the display under the cursor, with the default button in blue; they show without taking the keyboard, which stays with the app he works in (0524), and Enter and Escape pick the default once he clicks in. A toast window is the page's toast row with the lift, 380 × 84 at the display's bottom-right, newer ones stacked above, on top and without the keyboard; a click focuses its pane, its × dismisses it. The service's startup error ("The Jocasta service didn't start.") stays Windows' own box: no page is served then.

### The Canvas (whiteboard)
- **Surface:** whiteboard's canvas as it ships (0393), filling the canvas side under the canvas bar, with one scroll area (its own); its menus are small bg-2 popovers.
- **Scope:** the page's element rules (type, links, buttons and their 180ms colour transition, focus, scrollbars, reduced motion) apply only inside Jocasta's own parts (the rail, the canvas bar, the empty state, the source panel, menus, the capsule), never inside whiteboard's canvas, whose buttons keep their own timing.
- **Code:** Monaco on bg-0 (#1a1b26) with the shared code palette, Geist Mono 12.5/19, line numbers in fg-3, read-only, no minimap (0411).
- **Source panel:** bg-0, a line-2 border, 10px corners and the Lift, over the right ~70% of the canvas; the lit range has a blue left rule.

## Do's and Don'ts

### Do:
- **Do** take every colour from Tokyo Night as herdr draws it, and change it only in `src/frontier/ui/palette.css` (0474); no other file writes a colour out.
- **Do** keep blue as light: lines, marks of a few pixels, short text, and washes at 10%.
- **Do** use the state colours only as herdr means them: yellow working, red blocked or failed, teal done, green idle; yellow's one other use is what wants a look soon (a wait, an overlap, quota past 80%, an unpushed leftover; 0456).
- **Do** separate regions with hairlines and adjacent ground steps.
- **Do** set the greeting in Mona Sans 300 at 112.5% width; everything else at regular width.
- **Do** use Geist Mono only for code, paths, keys, cost, ids and times, with tabular numerals for live figures.
- **Do** let new things arrive once with the resolve motion and stop; loop only while a state holds.
- **Do** keep toasts as rows at the rail's foot, leaving the canvas side to the canvas.
- **Do** send anything about a thread to herdr (focus its pane); the companion shows, it doesn't drive threads.
- **Do** honour reduced motion by ending every motion at once.

### Don't:
- **Don't** bring thread tabs, terminals, a chat, request cards or pills back into the window (0409); they live in herdr.
- **Don't** fill any surface larger than a mark with blue.
- **Don't** use tracked or uppercase labels, kickers or eyebrows.
- **Don't** add corner ticks or HUD ornament.
- **Don't** leave glow resting on anything.
- **Don't** animate anything while idle, and don't slide anything in from an edge.
- **Don't** put a lift on anything that isn't asking him something.
- **Don't** add a theme setting; there is one look.
