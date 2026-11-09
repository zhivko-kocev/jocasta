---
version: 1
slug: "src-frontier-ui-index-html"
primary_target: "src/frontier/ui/index.html"
related_targets: []
---

# Frontier main window

Scope: Frontier's one window (HUD, Jocasta's column, the canvas or a thread). Visitor mode: Operate.
Audience: the owner, all day at a Windows desktop, talking to Jocasta by voice or text, watching threads, answering what blocks them.
Constraints: Electrobun webview, strict CSP (fonts from the page's own origin), offline, motion only when something changes (0258, 0292), thread tabs are the real harness TUI (0357).
Built in `src/frontier/ui` under records 0369 (look), 0370 (layout) and 0371 (canvas motion), after a prototype outside the repo.

## Direction contract

THESIS: Jocasta presents her crew. The window is a conversation with a composed, British intelligence: her presence and greeting first, the threads introduced as pills beneath her, the canvas or the chosen thread on the right. It refuses the dashboard of panels and the noisy sci-fi HUD.

OWN-WORLD: Midnight ink (#06090f to #131c29) with blue-tinted hairlines; cyan #92d6ec only as light (lines, marks, short text, washes under 10%); champagne #d9c08f for "awaiting you", rose #e39a9a for failures, sage #9cc9b0 for landed. Mona Sans (wide light for the greeting and wordmark, regular for prose), Geist Mono for code and data; thread terminals keep Cascadia Mono in the look's ANSI palette. Pills, raised docked requests, a two-ring Presence.

STORY: He sees who is at work and what needs him in the greeting and the pills, answers the docked request where he is, hears Jocasta and watches the spoken line brighten and underline, and watches the canvas draw by hand when he opens it.

FIRST VIEWPORT: HUD 46px across. Left (about 880px at 1440): the 64px Presence and "Good evening." at 30px light, the summary sentence; the crew under it (Canvas, a pill per thread with its state, Projects); the conversation; the docked requests above the composer; one hint line. Right (39vw, about 560px): the canvas with its drawing and code peek, or a thread's status strip and TUI. Primary action: the docked request's answer, then the composer.

FORM: Tabs under the presence, third of seven (re-roll 1 steered by Zhivko: presence, Jocasta left, canvas right, threads as tabs). Seed key 7fa50927. Signature: the canvas drawn by a point of light with ink that dries; the spoken line marked while Jocasta speaks.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Memorable moment
A diagram drawing itself on the right while Jocasta, on the left, speaks and the line she is saying lights up.

## Decided since the prototype
- One look, no theme setting; fonts bundled unmodified under the OFL (0369).
- The crew, the projects sheet, the greeting's summary and Ctrl+1–9 (0370). Ctrl J was not built: it wasn't in the sketch he locked.
- Per-step narration was not built: a drawing's steps carry no spoken text.
