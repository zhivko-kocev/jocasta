---
name: Jocasta
description: "Tokyo Night everywhere Jocasta draws: the `today` pane and the `ask` popup in herdr, drawn with OpenTUI, and the canvas page in his browser around whiteboard's canvas."
colors:
  bg-0: "#1a1b26"
  bg-1: "#232636"
  bg-2: "#24283b"
  bg-3: "#2d3650"
  fg: "#c0caf5"
  fg-2: "#a9b1d6"
  fg-3: "#8a93bf"
  mark-3: "#697196"
  comment: "#565f89"
  bright-black: "#414868"
  blue: "#7aa2f7"
  cyan: "#7dcfff"
  yellow: "#e0af68"
  red: "#f7768e"
  green: "#9ece6a"
  purple: "#bb9af7"
  orange: "#ff9e64"
  spoken: "#e5e9fb"
  line-1: "rgb(86 95 137 / 22%)"
  line-2: "rgb(86 95 137 / 40%)"
  line-3: "rgb(86 95 137 / 65%)"
  blue-wash: "rgb(122 162 247 / 10%)"
  blue-line: "rgb(122 162 247 / 40%)"
  blue-find: "rgb(122 162 247 / 30%)"
  blue-select: "rgb(122 162 247 / 24%)"
  blue-hover: "rgb(122 162 247 / 6%)"
  blue-press: "rgb(122 162 247 / 5%)"
  red-line: "rgb(247 118 142 / 38%)"
typography:
  body:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.55
  heading:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 500
    lineHeight: 1.4
  lead:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14.5px"
    fontWeight: 400
    lineHeight: 1.5
  bar:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 500
    lineHeight: 1.4
  small:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.4
  meta:
    fontFamily: "Mona Sans, Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.4
  code:
    fontFamily: "Geist Mono, Cascadia Mono, Consolas, monospace"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: "19px"
  mono-path:
    fontFamily: "Geist Mono, Cascadia Mono, Consolas, ui-monospace, monospace"
    fontSize: "12.5px"
    fontWeight: 400
    lineHeight: 1.4
  mono-data:
    fontFamily: "Geist Mono, Cascadia Mono, Consolas, ui-monospace, monospace"
    fontSize: "12px"
    fontWeight: 400
rounded:
  item: "4px"
  s: "6px"
  m: "10px"
components:
  today-ground:
    backgroundColor: "{colors.bg-0}"
  today-heading:
    textColor: "{colors.blue}"
    fontWeight: bold
  today-picked-row:
    backgroundColor: "{colors.bg-2}"
    marker: "› in {colors.blue}"
  today-quiet:
    textColor: "{colors.comment}"
  ask-ground:
    backgroundColor: "{colors.bg-0}"
  ask-title:
    textColor: "{colors.blue}"
    fontWeight: bold
  ask-choice:
    backgroundColor: "{colors.bg-2}"
    textColor: "{colors.fg}"
  ask-choice-picked:
    backgroundColor: "{colors.blue}"
    textColor: "{colors.bg-0}"
    fontWeight: bold
  ask-code:
    textColor: "{colors.yellow}"
    fontWeight: bold
  canvas-page:
    backgroundColor: "{colors.bg-1}"
  canvas-bar:
    backgroundColor: "{colors.bg-0}"
    textColor: "{colors.fg-2}"
    typography: "{typography.bar}"
    padding: "8px 14px"
  button:
    backgroundColor: "transparent"
    textColor: "{colors.fg}"
    rounded: "{rounded.s}"
    padding: "6px 12px"
  button-hover:
    backgroundColor: "{colors.blue-press}"
  button-small:
    typography: "{typography.small}"
    padding: "3px 9px"
    rounded: "{rounded.s}"
  empty-state:
    padding: "28px"
  source-panel:
    backgroundColor: "{colors.bg-0}"
    rounded: "{rounded.m}"
  source-head:
    backgroundColor: "{colors.bg-1}"
    padding: "8px 12px"
  menu:
    backgroundColor: "{colors.bg-2}"
    rounded: "{rounded.s}"
    padding: "4px"
  menu-item:
    textColor: "{colors.fg}"
    typography: "{typography.small}"
    rounded: "{rounded.item}"
    padding: "6px 10px"
---

# Design System: Jocasta

## Overview

Jocasta has no window of its own (0552). What it draws is three surfaces, all on one Tokyo Night palette:

- **the `today` pane**, a terminal pane beside the boss in herdr's workspace "Jocasta", drawn with OpenTUI (0551, 0553, 0555);
- **the `ask` popup**, herdr's popup in front of whatever he is on, also OpenTUI, for his questions (0553, 0558, 0568);
- **the canvas page**, a plain-DOM page the service serves to his browser around whiteboard's canvas (0393, 0552).

herdr itself draws everything else with its own `tokyo-night` theme, without custom colours (0554); omp's panes use omp's `dark-tokyo-night` (0401). The terminals inherit the Windows Terminal scheme `jocasta setup` writes from the same palette.

The two terminal surfaces are coloured text only: OpenTUI draws plain lines of coloured runs (`src/frontier/today/view.ts`, `src/frontier/ask/view.ts`) on bg-0. There are no borders, boxes or motion; layout is lines, blank lines and the terminal's width.

## Colors

`src/frontier/canvas/palette.css` is the one palette file (0474): Tokyo Night as herdr 0.9.5 draws it, as `:root` custom properties, and the only file with its hex values; translucent colours are built from them (`rgb(from var(--blue) r g b / 10%)`). The canvas page links it first, so its first paint is bg-0 (0401). `src/frontier/canvas/palette.ts` reads its hex values for everything CSS can't colour: the `today` pane, the `ask` popup, the canvas's Monaco theme and the Windows Terminal scheme. The front matter above mirrors it. `--spoken` is still in the file, and nothing uses it.

### Grounds
- **BG 0** (bg-0): the `today` pane's and the `ask` popup's ground, the canvas bar, the source panel, Monaco's editor and gutter, the terminal scheme's background, and the page's first paint.
- **BG 1** (bg-1): the canvas page's body, the "couldn't start" screen, the source panel's head and a diff file's head; Monaco's unchanged regions.
- **BG 2** (bg-2): the picked thread row in `today`, the active field and the unpicked choices in `ask`, the canvas's menus, Monaco's widgets; the terminal scheme's black.
- **BG 3** (bg-3): the terminal scheme's selection.

### Text
- **FG** (fg): primary text everywhere: the date and time in `today`'s header, thread names, a question's message, the canvas's titles and buttons.
- **FG 2** (fg-2): secondary text: CPU and memory, quota below 80%, a thread's time and cost, LANDED TODAY rows without a pane, headlines, a question's detail, the canvas bar, the empty state's sentence, Monaco's delimiters and operators.
- **FG 3** (fg-3): quiet text on the canvas page: notices, the source panel's meta line, the folded Details; lifted from herdr's overlay1 so it reaches 4.5:1 on bg-0 to bg-2 (0432).
- **Comment** (comment): quiet lines in the terminal surfaces ("nothing waits on you", "no thread at work", wait times, an idle thread's state, news feed names, the key lines and hints), Monaco's comments (italic), and the colour every canvas hairline is made from.
- **Mark 3** (mark-3): Monaco's line numbers.
- **Bright Black** (bright-black): the terminal scheme's bright black only.

### Accents
- **Blue**: headings in `today` (NEEDS YOU, UNDERWAY, LANDED TODAY, NEXT, NEWS), the picked row's `›`, the `ask` popup's title and the picked choice's ground; on the canvas page links, focus outlines, the caret, the lit range's left rule, a checked menu item's ✓, and Monaco's cursor.
- **Cyan**: the weather in `today`'s header, a calendar entry's time, New thread's project; Monaco's types.
- **Yellow**: a NEEDS YOU row that isn't blocked or reviewed, the code to type in `ask` and the letters typed so far, the line "Jocasta's service isn't answering; trying again…", a popup with no question.
- **Orange**: a reviewed thread in NEEDS YOU; Monaco's numbers and string escapes.
- **Red**: a blocked thread in NEEDS YOU, a quota window at 80% or more, a key that failed in `today`, a note under a question (a wrong code, "Write the brief first."); on the canvas page error notices and the raw error line; deleted lines in diffs.
- **Green**: today's spend, a working thread's state, a key that worked in `today`; Monaco's strings; added lines in diffs.
- **Purple**: a thread's context size in UNDERWAY; Monaco's keywords and tags.

### Translucent
- **Line 1 / 2 / 3** (comment at 22%, 40%, 65%): line 1 under the canvas bar and the source panel's and diff file's heads; line 2 around the source panel, menus and diff files, and the page's thin scrollbars; line 3 a button's resting border, so it reads against the ground (0432).
- **Blue Wash** (10%): a counted or lit range, a menu item's hover. **Blue Find** (30%): find matches. **Blue Select** (24%): text selection in Jocasta's own parts. **Blue Hover** (6%) and **Blue Press** (5%): a button's hover. **Blue Line** (40%): a link's underline.
- **Red Line** (38%): in the palette; no canvas rule uses it now.

### Monaco
Monaco's `tokyo-night` theme is built in `src/frontier/canvas/monaco.ts` from the palette (0411, 0474): text fg, keywords purple, strings green, numbers orange, types cyan, comments italic in comment, errors red; selection and find highlights are blue at 20% and 40%, a range at 10%; inserted and removed text green and red at 20%, their lines at 8%. Code colours carry syntax only.

### Windows Terminal
The scheme "Tokyo Night" `jocasta setup` writes (0401, 0474): background bg-0, foreground and cursor fg, selection bg-3; black bg-2, white fg-2, bright black bright-black, bright white fg; red, green, yellow, blue, purple and cyan, each the same in its bright form.

## The `today` pane

`today` (0555) is lines of text, top to bottom:

1. **The header**, flowed over as many lines as the width needs, parts joined with " · ": the date and time (`Mon 9 Nov 14:20`, bold fg), the weather (`12° Light rain`, cyan; only with `dashboard.weather` set), `cpu N% · mem N%` (fg-2), each quota window (`5h N%`, fg-2, red from 80%) and today's spend (`$3.20 today`, green). While the service doesn't answer, the header is one yellow line instead.
2. **NEEDS YOU**, **UNDERWAY**, **LANDED TODAY** (only when something landed) and **NEXT**, each an uppercase bold blue heading over its rows. An empty section says so in one comment-coloured line. A NEEDS YOU row is coloured by kind (blocked red, reviewed orange, the rest yellow) with how long it has waited flush right; an UNDERWAY row is the thread's `project/title` in fg with its state, time, cost and context flush right; a NEXT row is the time in cyan and the title in fg.
3. **NEWS**, only when at least three rows are left: a blank line, the heading, then one headline per row, its feed in comment and its title in fg-2.
4. **The foot**: what his last key did (green, or red when it failed), then the key line `↑↓ thread · Enter go to it · f Finish · s Stop` in comment.

The picked thread carries a blue `›` and its whole row is filled bg-2. When the sections don't fit, they scroll so the picked thread stays in view. Any line longer than the pane is cut with "…"; a row with something flush right cuts its left part first.

## The `ask` popup

The popup (0553) is herdr's: 70% wide and 18 rows high (the plugin's manifest), titled "Jocasta asks". Inside, OpenTUI draws: the question's title in bold blue ("Jocasta: Allow or Deny", "Jocasta: Stop everything?", "Jocasta: Finish <thread>", "Jocasta: New thread"), a blank line, the message wrapped in fg, the detail wrapped in fg-2, and then:

- **Choices**: one row of buttons, each ` <choice> ` with two spaces between; the picked one bold bg-0 on blue, the others fg on bg-2.
- **A coded choice** (Allow, Merge, PR, Stop everything; 0558, 0568): "Type `XYZ` for <choice>" with the code in bold yellow, then `> ` and the letters typed so far, padded with `_`, in bold yellow; the hint `←→ choose · Esc <fallback>` in comment.
- **An uncoded choice**: the hint `←→ choose · Enter <choice> · Esc <fallback>` in comment.
- **New thread**: `Project  ‹ name ›` (the label in comment, the project bold cyan) and `Brief    <text>▏` (fg), the active field on bg-2, then `Tab field · ↑↓ project · Enter start · Esc cancel` in comment.
- **A note** under everything in red when something went wrong ("Wrong code; nothing was done.").

## The canvas page

The page (`src/frontier/canvas/`, 0552) opens in his browser at the service's address, at `/?review=<id>` from the boss's `show` or at `/` for whiteboard's Home. Its ground is bg-1, and whiteboard v0.2.4's own canvas fills it with its own look and its own scroll area (0393). Jocasta's parts around it:

- **The canvas bar**, only while a review is open (0432): bg-0 with a line-1 hairline under it, 8px 14px, bar type in fg-2: a small "Reviews" button (back to whiteboard's Home) and the review's title in fg, cut with an ellipsis. The tab's title is `<review> · Jocasta`.
- **The empty state**, instead of whiteboard's Welcome until the first review exists (0419): centred, 28px padding, "Nothing on the canvas yet." in heading type over one lead-type sentence in fg-2 (at most 44ch), and a "Take the whiteboard tour" button 14px below.
- **The "couldn't start" screen** (0432): the same centred layout on bg-1 over the stage, "The canvas couldn't start.", the reason and what to do (at most 52ch), Try again only when it can help, and the raw error folded under Details (12.5px fg-3, the code in 12px mono fg-2).
- **The source panel**, opened from a step's code: over the stage from 30% of its width to 24px from the right, top and bottom; bg-0 with a line-2 border, 10px corners and the Lift. Its head (bg-1, line-1 under it, 8px 12px) holds the path in mono-path type, the meta line ("lines a–b · after the change" or "· before the change") in fg-3 and a small Close; Escape closes it too.
- **Code peeks and diffs**: Monaco 0.57.0, read-only, Geist Mono 12.5px on 19px lines, no minimap (0411); a peek has 6px corners; a diff file has a line-2 border, 6px corners and a bg-1 head in 12px mono with the counts in green and red; a lit range is blue-wash with a 2px blue left rule.
- **Menus** the canvas asks for: bg-2, line-2 border, 6px corners, 4px padding, the Lift, at least 160px wide; items 6px 10px at 12.5px with 4px corners, blue-wash on hover, a blue ✓ before a checked one.

### Buttons
Quiet by default, never filled: transparent with a line-3 border, fg text, 6px 12px, 6px corners; hover brings the border to fg-3 over blue-press. Small buttons are 3px 9px at 12.5px. Focus is a 1px blue outline offset 2px; disabled is 40% opacity.

### Type
Mona Sans (variable, weight 200 to 900, width 75% to 125%) and Geist Mono (variable), both bundled as woff2 with their OFL licences in `src/frontier/canvas/fonts/` and served from the page's own origin. Jocasta's parts set body type (14px, 1.55) in fg; mono is used for paths, diff heads, raw errors and Monaco. The page's element rules (type, links, buttons, focus, scrollbars) are scoped to Jocasta's own parts, never inside whiteboard's canvas.

### Motion
`:root` holds one ease (`cubic-bezier(0.22, 1, 0.36, 1)`) and two durations: quick 180ms for a button's colours and border, calm 420ms for arrivals. The empty state and the "couldn't start" screen arrive once with `resolve` (fade, 4px rise, 4px blur clearing). Under `prefers-reduced-motion: reduce` every animation and transition of Jocasta's parts ends at once (1ms, one iteration); whiteboard's canvas keeps its own reduced-motion design.

### Elevation
Flat by default. The Lift (`0 1px 0 rgb(255 255 255 / 3%) inset, 0 10px 30px -14px rgb(0 0 0 / 75%), 0 2px 6px -2px rgb(0 0 0 / 50%)`, `--lift` in the palette) is on the source panel and the canvas's menus only.

## Do's and Don'ts

### Do:
- **Do** take every colour from `src/frontier/canvas/palette.css` (0474), through `palette.ts` outside CSS; no other file writes a colour out.
- **Do** keep the terminal surfaces to coloured text on bg-0, as OpenTUI lines.
- **Do** separate the canvas page's regions with line-1 hairlines and adjacent ground steps.
- **Do** keep Jocasta's page rules inside its own parts, leaving whiteboard's canvas its own look.
- **Do** honour reduced motion by ending every motion of Jocasta's parts at once.

### Don't:
- **Don't** add a greeting or the Presence to `today` (0555).
- **Don't** let Enter take a coded choice in `ask`: Allow, Merge, PR and Stop everything take their typed code (0558, 0568).
- **Don't** fill a canvas button; blue on the canvas page is a line, a mark, text or a wash of 10% to 30%, and the only solid blue ground is the picked choice in `ask`.
- **Don't** add a theme setting; there is one look.
