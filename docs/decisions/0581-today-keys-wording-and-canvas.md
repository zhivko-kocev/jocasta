# 0581 today, the keys, the wording and the canvas

- Status: v1
- Kind: product, ux
- Phase/slice: program
- Vision: V4, V6, V11, V12
- Date: 2026-11-09

## Question
The review found that `today` breaks at narrow widths, key actions give no feedback, his own keys raise notifications, links can't be opened, texts written for the boss reach him, the canvas after a restart says "Forbidden", and a non-Latin brief can't start a thread. How does each change?

## Facts
Verified 2026-11-09 by the review (0573), reports UX, SVC (SVC-19) and FE (FE-8).
- At 30–40 columns `today` cuts titles to the project, drops states and wraps the header over 3–4 rows (`today/view.ts:65-73`); kinds are told by colour only (orange vs yellow 1.02:1 in luminance).
- A key action reports once, when it ends; a New thread whose brief wasn't sent shows green ("Started … but omp didn't come up …", `service.ts:3765`).
- Questions his own key opened raise "Jocasta asks" with the request sound (`plugin.ts:244-250`); rows with a `url` are drawn but never pickable.
- Key and CLI results say "his approval", "he runs gh auth login", "He chose Deny"; errors from his form start `thread_start:`.
- After a restart the canvas page gets a bare `Forbidden` (`server.ts:649`), and Try again says "whiteboard's server didn't start" for a 403, a fetch failure and a failed stylesheet alike.
- `branchSlug` keeps only `a-z0-9`: a Macedonian brief gives an empty slug and the refusal "thread_start: the title needs letters or digits for its branch.", with the brief lost.

## Options
| Item | Options (pros / cons) |
|---|---|
| UX-4 narrow today | A. priorities, glyphs, yielding header · B. two-line rows under 45 columns (half as many threads) |
| UX-5 feedback | A. start line, half-done in red with a notification, outcome gone after a minute · B. A with the outcome kept · C. keep |
| UX-6 own-key toasts | A. none · B. without sound · C. keep |
| UX-7 links | A. Enter opens a picked link row · B. a separate `o` key · C. keep |
| UX-8 + SVC-19 wording | A. wording by door, the door in error prefixes · B. second person everywhere (ambiguous for the boss) · C. keep |
| UX-9 + FE-8 canvas | A. a friendly page and the right trouble texts · B. A plus a door to Home (its own record) · C. keep |
| UX-11 slugs | A. transliterate, dated fallback · B. dated fallback only · C. check in the form |
| UX-12–18 | the reports' fixes · keep |

## Recommendation
A for every item; UX-12–18 as the reports give them.

## Choice
- UX-4 A: at narrow widths the title keeps at least 18 columns, then context, cost and time drop in that order (state last); weather, then CPU and memory, yield when the header would pass two rows; a one-glyph kind marker (`?` a question or plan, `✓` reviewed, `↗` review request, `!` red pipeline or disk; reminders keep their time); blocked rows get `since` from `changedAt`; a red quota shows its reset (`5h 91% (16:30)`).
- UX-5 A: key actions report a start line at once ("Starting a thread in acme-api: worktree, omp, brief…", "Merging acme-api/x into main…"); thread start and squash return `{text, isError}` so half-done outcomes are red and notify; the outcome wraps over up to two rows and is dropped after a minute.
- UX-6 A: questions his own key opened raise no notification (every Allow, and the close question after the terminal closed, keep theirs).
- UX-7 A: rows with a `url` become pickable and show `↗`; Enter on one opens it (`{type: "link.open", url}`), only when it is one of the service's own current today/dashboard URLs and http(s); the key line reads `↑↓ pick · Enter go/open · f Finish · s Stop`.
- UX-8 A + SVC-19 (a): wording by door: the key and the CLI get "you/your" and no tool names, the boss keeps "he/his"; the subject is always `project/title`; error prefixes name the door (`New thread:`, `jocasta thread new:`, `thread_start:`).
- UX-9 A + FE-8 A: a browser navigation without a valid cookie gets a small Tokyo Night page "This Jocasta page is from an earlier run. Ask the boss to show the review again; it opens a fresh page." (no token in it). `canvasTrouble` maps 401/403 to "This page is from an earlier run of Jocasta. Ask the boss to show the review again." (not transient), a fetch TypeError to "Jocasta isn't running. Run jocasta, then ask the boss to show it." (transient), a stylesheet failure to "Jocasta's whiteboard files didn't load. Try again; if it keeps failing, restart Jocasta.", and a missing mount function to "Jocasta's whiteboard files don't match this version. Reinstall Jocasta; in a development checkout, run bun run build." (no Try again). The failed `<link>` is removed before rejecting, and Try again after a failed script import reloads the page.
- UX-11 A: branch slugs transliterate (a Macedonian/Cyrillic table: ѓ→gj, ж→zh, ќ→kj, љ→lj, њ→nj, ѕ→dz, џ→dzh, ч→ch, ш→sh, …; NFKD with combining marks dropped for Latin); an empty slug falls back to `thread-<YYYYMMDD>-<n>`; the title keeps his words.
- UX-12: quiet text `P.fg3`, separators `P.comment`; DESIGN.md's Comment and FG 3 lines follow.
- UX-13: the HUD reads `N awaiting you · $X today`, N being NEEDS YOU's rows; one shared "not running" text, "Jocasta isn't running; start it with jocasta."; the close question ends "…They pick up where they left off next time."
- UX-14: a CALENDAR heading for meetings, NEXT for his notes after the calendar while rows remain; a started meeting stays as `now` until its end.
- UX-15: the New thread form opens on Brief with a project always picked (the focused one, else the last started in, else the first); the caret only in the focused field; type-to-find in Project; a hint per field.
- UX-16: `r` Review and `n` New thread in `today`; the key line drops lower keys by width; the refusal reads "Finish: in today, press f on the picked thread."; empty states hint `n starts one`; herdr.toml's description says "run".
- UX-17: "Connecting to Jocasta…" until the first connect; offline keeps the time and says since when; a `quiet` header part; a red header part for a down service with its note; error replies and Enter while offline shown in the foot.
- UX-18: an unknown command is named first; nested usages without the "jocasta: " prefix; an empty `project list` says how to add one; USAGE names `check`: `jocasta mcp [--boss]   the MCP server omp starts (graph_search, report_result and check; the boss's tools with --boss)`.
  As built: the USAGE line keeps its column with the others (`jocasta mcp [--boss]     the MCP server…`, five spaces); the words are the record's.

## Reason
"A" for UX-4 to UX-11 (with SVC-19 "(a)" and FE-8 "A"); UX-12 to UX-18 ticked.

## Consequences
- Amends 0403 (the HUD), 0414/0445 (branch slugs), 0429 (today's rows and links), 0432 (texts by door), 0442, 0544 (the CLI's wording), 0548, 0551, 0554 (the key line and keys) and 0555.
