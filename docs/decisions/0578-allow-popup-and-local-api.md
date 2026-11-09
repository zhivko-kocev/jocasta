# 0578 The Allow popup and the service's local API

- Status: v1
- Kind: product, security
- Phase/slice: program
- Vision: V3, V11
- Date: 2026-11-09

## Question
The review found that the typed code can be read over HTTP, a long question pushes the code out of the popup, typing into a fresh popup spends the tries, pane identity is self-asserted, and questions and toasts have no dedupe. How do the popup and the local API change?

## Facts
Verified 2026-11-09 by the review (0573), reports SEC-NET, FE and UX.
- The popup's code sits in its environment (`plugin.ts:237`), and `GET /api/popup/<code>` returns the whole view, letters included (`server.ts:675-677`); any process holding the token and that code can answer Allow.
- `askLines` has no height (`ask/view.ts:134-189`) and the popup is 18 rows: a 5-line brief at 84×18 puts the code prompt off screen; `wrap` slices a long word with no marker; no redraw on resize.
- Typing "then update…" into a fresh Allow popup sent three wrong codes and ended as Deny; the boss heard "He chose Deny"; a Cyrillic layout types nothing; Esc is dropped while posting; pastes keep escape sequences (FE-11).
- Pane identity is the body's `paneId` (`entry.ts:95`, `server.ts:597`), which any process can claim for `graph_search`, `report_result` and `check`.
- Finish and New thread questions wait unbounded, queue oldest first with no dedupe, and every refusal raises a toast.

## Options
| Item | Options (pros / cons) |
|---|---|
| SEC-NET-1 reading the code | 1. docs only (honest / defeatable) · 2. one reader per question, plus 1 (a late reader kills the question / a narrow race) · 3. letters never leave the popup (more parts, same race) |
| Popup fit (FE-1, UX-1, SEC-NET-2) | A. height-aware popup, scrollable detail (code always visible / two keys) · B. taller popup only (still overflows) · C. A and B |
| Typed code (UX-2, FE-11) | A. grace period, "one try left", the real reason to the boss (no new keys) · B. A plus Enter to submit |
| SEC-NET-6 pane identity | 1. a per-pane key as for the boss (one pattern / restored panes need a restart) · 2. document it |
| SEC-NET-8 questions and toasts | 1. one open question per (action, thread), bounded waits, Allow first, toast rate limit · 2. dedupe only |

## Recommendation
SEC-NET-1 options 1 and 2; A; A; option 1; option 1.

## Choice
- SEC-NET-1: one reader per question. The first `GET /api/popup/<code>` returns the view with the letters and a reader secret; hold and answer must carry it. A second GET or hold for an open question ends it as its fallback and raises a warning toast: "Jocasta — Another program read your question "…"; it counted as Deny." The README and the thesis say what the code does and doesn't stop.
- Popup fit: `askLines` gets a height; the bottom block (buttons, code prompt and echo, key line, note) is laid out first; the detail gets the rows left, ending with `… N more lines · PgDn reads on`; PgUp/PgDn scroll it; long words hard-wrap instead of being cut; it redraws on resize; the popup keeps 18 rows.
- Typed code: keys are ignored for about 750 ms after the popup first draws; after the 2nd wrong code the note says "Wrong code; one try left, then it counts as Deny."; the 3rd wrong code shows "Wrong code three times; Deny was taken." in red for 2 s (served as a distinct result, e.g. 410 `exhausted`) before closing; a non-Latin letter on a coded choice shows "Type the Latin letters KQX (switch the keyboard layout)."; the popup records how it ended, and the boss is told the true reason ("Three wrong codes, which count as Deny; nothing was merged.", "The popup closed without an answer, which counts as Deny; …"). Internal parts (FE-11): an Esc pressed while posting is queued; pastes are decoded and stripped of escapes.
- SEC-NET-6: a per-pane key. Each launched pane gets `JOCASTA_PANE_KEY` (random, kept with the launched panes); `jocasta mcp` sends it; the service maps key → pane and ignores the body's `paneId`; panes restored from an older run need a restart, as with the boss key.
- SEC-NET-8: at most one open question per (action, thread): a repeat brings the open one to the front, and `today` says "Finish: already asking about acme-api/retry."; Finish and New thread wait `CONFIRM_WAIT_MS` (2 min) like Allow; Allow questions go ahead of key questions; a refusal toast shows at most once per text per 30 s (`notifications.jsonl` still logs each).

## Reason
"Options 1 and 2" for SEC-NET-1; "A" for the popup fit and the typed code; "option 1" for SEC-NET-6 and SEC-NET-8.

## Consequences
- Amends 0484 (F2: pane identity is the per-pane key, not the body's `paneId`), 0485 (the accepted-risk text names what the code stops), 0552, 0553, 0554, 0558 and 0568 (the popup's layout, the typed code's edges and the question queue).
- The plan's appendix I gains `JOCASTA_PANE_KEY` and the popup's reader secret on `/api/popup/<code>`.
