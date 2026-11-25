---
name: Jocasta
description: "Tokyo Night everywhere Jocasta draws: the `today` dock and the question box in Jocasta's shell, drawn with ratatui, and the brief pages in his browser."
colors:
  bg-0: "#1a1b26"
  bg-1: "#232636"
  bg-2: "#24283b"
  bg-3: "#2d3650"
  fg: "#c0caf5"
  fg-2: "#a9b1d6"
  fg-3: "#8a93bf"
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
  blue-select: "rgb(122 162 247 / 24%)"
  blue-hover: "rgb(122 162 247 / 6%)"
  blue-press: "rgb(122 162 247 / 5%)"
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
    textColor: "{colors.fg-3}"
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
  brief-page:
    backgroundColor: "{colors.bg-1}"
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

- **the `today` dock**, beside the boss in the shell's workspace "Jocasta", drawn with ratatui (0551, 0553, 0555, 0634, 0683);
- **the question box**, the shell's own box in front of whatever he is on, also ratatui, for his questions (0553, 0558, 0568, 0683);
- **the brief pages**, plain-DOM pages the service serves to his browser: the thread brief, the boss's `show` and the later briefs (0678, 0679).

The shell draws everything else with its `tokyo-night` theme (0554, 0683); omp's panes use omp's `dark-tokyo-night` (0401). The terminals inherit the Windows Terminal scheme `jocasta setup` writes from the same palette.

The two terminal surfaces are coloured text only: ratatui draws plain lines of coloured runs (`src/frontier/today/view.rs`, `src/frontier/ask/view.rs`; 0634) on bg-0. There are no borders, boxes or motion; layout is lines, blank lines and the terminal's width.

## Colors

The palette is Tokyo Night as the shell draws it (the same as herdr 0.9.5's, 0554). The brief pages' `:root` in `src/frontier/brief/page/brief.css` holds its hex values for the browser (0678); the terminal surfaces and the Windows Terminal scheme `jocasta setup` writes take theirs from `src/frontier/today/view.rs` and setup. The front matter above mirrors it.

### Grounds
- **BG 0** (bg-0): the `today` pane's and the `ask` popup's ground, a brief page's first paint, and the terminal scheme's background.
- **BG 1** (bg-1): a brief page's panels and a diff file's head.
- **BG 2** (bg-2): the picked thread row in `today`, the active field and the unpicked choices in `ask`, the terminal scheme's black.
- **BG 3** (bg-3): the terminal scheme's selection.

### Text
- **FG** (fg): primary text everywhere: the date and time in `today`'s header, thread names, a question's message, a brief's titles.
- **FG 2** (fg-2): secondary text: CPU and memory, quota below 80%, a thread's time and cost, LANDED TODAY rows without a pane, headlines, a question's detail.
- **FG 3** (fg-3): quiet text: in the terminal surfaces "nothing waits on you", "no thread at work", wait times, an idle thread's state, news feed names, `quiet` in `today`'s header, "Connecting to Jocasta…", the key lines and hints, the popup's "… N more lines" line and the form's labels (0581). Lifted from herdr's overlay1 so it reaches 4.5:1 on bg-0 to bg-2 (0432).
- **Comment** (comment): the " · " separators in the terminal surfaces (0581), and the colour a brief's hairlines are made from.
- **Bright Black** (bright-black): the terminal scheme's bright black only.

### Accents
- **Blue**: headings in `today` (NEEDS YOU, UNDERWAY, LANDED TODAY, CALENDAR, NEXT, NEWS), the picked row's `›`, a link row's `↗`, the `ask` popup's title and the picked choice's ground; on a brief page links, focus outlines and an active flow node.
- **Cyan**: the weather in `today`'s header, a calendar entry's time (or `now`), New thread's project.
- **Yellow**: a NEEDS YOU row that isn't blocked or reviewed, the code to type in `ask` and the letters typed so far, "Jocasta isn't answering (since 18:03); trying again…", a popup with no question.
- **Orange**: a reviewed thread in NEEDS YOU.
- **Red**: a blocked thread in NEEDS YOU, a quota window at 80% or more, a broken service in `today`'s header, a key that failed or an error reply in `today`, a note under a question (a wrong code, "Write the brief first.", "Wrong code three times; Deny was taken."); on a brief page a failed node, a failed check and deleted lines in diffs.
- **Green**: today's spend, a working thread's state, a key that worked in `today`; added lines in diffs.
- **Purple**: a thread's context size in UNDERWAY.

### Translucent
- **Line 1 / 2 / 3** (comment at 22%, 40%, 65%): line 1 under a diff file's head; line 2 around a diff file and the page's thin scrollbars; line 3 a button's resting border, so it reads against the ground (0432).
- **Blue Wash** (10%): a focused flow node. **Blue Select** (24%): text selection in Jocasta's own parts. **Blue Hover** (6%) and **Blue Press** (5%): a button's hover. **Blue Line** (40%): a link's underline.

### Windows Terminal
The scheme "Tokyo Night" `jocasta setup` writes (0401, 0474): background bg-0, foreground and cursor fg, selection bg-3; black bg-2, white fg-2, bright black bright-black, bright white fg; red, green, yellow, blue, purple and cyan, each the same in its bright form.

## The `today` pane

`today` (0555, 0581) is lines of text, top to bottom:

1. **The header**, flowed over as many lines as the width needs, parts joined with " · ": the date and time (`Mon 9 Nov 14:20`, bold fg), `quiet` during quiet hours (fg-3), each broken service with its note (`herdr down (not answering)`, red), the weather (`12° Light rain`, cyan; only with `dashboard.weather` set), `cpu N% · mem N%` (fg-2), each quota window (`5h N%`, fg-2, red from 80%, with its reset when that is within a day: `5h 91% (16:30)`) and today's spend (`$3.20 today`, green). When the header would pass two rows the weather gives way, then CPU and memory. Until the feed first connects the header is "Connecting to Jocasta…" (fg-3); while the service doesn't answer it is the time, then "Jocasta isn't answering (since 18:03); trying again…" in yellow.
2. **NEEDS YOU**, **UNDERWAY**, **LANDED TODAY** (only when something landed) and **CALENDAR**, each an uppercase bold blue heading over its rows. An empty section says so in one fg-3 line; UNDERWAY's adds `· n starts one`. A NEEDS YOU row starts with its kind's glyph (`?` a question or plan, `✓` reviewed, `↗` a review request, `!` a red pipeline or a full disk; a reminder its time), is coloured by kind (blocked red, reviewed orange, the rest yellow), and has how long it has waited flush right. An UNDERWAY row is the thread's `project/title` in fg with its state, time, cost and context flush right; as the pane narrows the title keeps at least 18 columns while the context, then the cost, then the time give way, the state last. A CALENDAR row is the time in cyan (`now` for a meeting that started and hasn't ended) and the title in fg.
3. **NEXT**, his notes filed through the boss (`acme-api: look at the slow search endpoint (#7)`), only while rows are left after the calendar.
4. **NEWS**, only when at least three rows are left: a blank line, the heading, then one headline per row, its feed in fg-3 and its title in fg-2.
5. **The foot**: what his last key did, or an error reply (green, or red when it failed), over at most two rows and gone after a minute; then the key line `↑↓ thread · Enter go · f Finish · s Stop · r Review · n New` in fg-3, its last keys dropped when the pane is too narrow.

Up and down pick a row: a thread (Enter goes to its pane) or a row with a link (a meeting, a review request, a red pipeline, a headline; Enter opens it), which ends in a blue `↗` (a review request's glyph says it already). The picked row carries a blue `›` and its whole row is filled bg-2. When the sections don't fit, they scroll only as far as the picked row needs. Any line longer than the pane is cut with "…", measured in terminal columns (a wide character takes two); a row with something flush right cuts its left part first.

## The `ask` popup

The box (0553, 0683) is the shell's: 70% wide and 18 rows high (the plugin manifest's size, kept), titled "Jocasta asks". Inside, ratatui draws: the question's title in bold blue ("Jocasta: Allow or Deny", "Jocasta: Stop everything?", "Jocasta: Finish <thread>", "Jocasta: New thread"), a blank line, the message wrapped in fg, the detail wrapped in fg-2 (a word longer than the line is split, never cut), and then the foot. The foot is laid out first so it always shows (0578); a question taller than the rows left shows its first lines and `… N more lines · PgDn reads on` in fg-3, and PgUp/PgDn read it. The popup redraws when its size changes, and keys in its first 750 ms are dropped.

- **Choices**: one row of buttons, each ` <choice> ` with two spaces between; the picked one bold bg-0 on blue, the others fg on bg-2.
- **A coded choice** (Allow, Merge, PR, Stop everything; 0558, 0568): "Type `XYZ` for <choice>" with the code in bold yellow, then `> ` and the letters typed so far, padded with `_`, in bold yellow; the hint `←→ choose · Esc <fallback>` (with `PgUp/PgDn read` before Esc when the question scrolls).
- **An uncoded choice**: the hint `←→ choose · Enter <choice> · Esc <fallback>`.
- **New thread**: `Project  ‹ name ›` (the label in fg-3, the project bold cyan) and `Brief    <text>▏` (fg), the field the keys go to on bg-2 and the caret only in Brief when it has them; it opens on Brief. The hint follows the field: `Tab project · Enter start · Esc cancel` in Brief, `Type to find · ↑↓ project · Enter brief · Esc cancel` in Project.
- **A note** above the hint in red when something went wrong ("Wrong code; nothing was done.", "Wrong code; one try left, then it counts as Deny.", "Type the Latin letters XYZ (switch the keyboard layout)."); after the third wrong code "Wrong code three times; Deny was taken." shows for 2 s before the popup closes.

Hints are fg-3 with " · " separators in comment.

## The brief pages

A brief (`src/frontier/brief/`, 0678) opens in his browser at the service's address, `/brief/<id>`, from the Done toast or the boss's `show`. Its ground is bg, panels sit one step lighter, and text is fg with muted text in mut; the page has one script and one stylesheet, and no font, image or script from outside. Sections run top to bottom: a `flow` is a grid by layer with drawn edges; a `map` is columns with bars for added and removed lines; a diff shows red and green lines; a card shows its severity, the quoted code and whether the quote was found in the committed file.

### Type
The system UI font at 15px on a 1.55 line, monospace for code, paths and diffs.

### Motion
Nodes fade in layer by layer, each edge draws itself, an active node pulses, and Play, Prev and Next walk the nodes in order. All of it is CSS transitions and one animation; under `prefers-reduced-motion: reduce` every animation and transition ends at once and nodes and edges show in their final state.

## Do's and Don'ts

### Do:
- **Do** take the brief pages' colours from the `:root` of `brief.css`; no rule writes a hex value of its own.
- **Do** keep the terminal surfaces to coloured text on bg-0, as ratatui lines.
- **Do** separate a brief's regions with hairlines and adjacent ground steps.
- **Do** honour reduced motion by ending every motion at once.

### Don't:
- **Don't** add a greeting or the Presence to `today` (0555).
- **Don't** let Enter take a coded choice in `ask`: Allow, Merge, PR and Stop everything take their typed code (0558, 0568).
- **Don't** fill a button on a brief page; the only solid blue ground is the picked choice in `ask`.
- **Don't** add a theme setting; there is one look.
