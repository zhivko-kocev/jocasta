# 0519 The close question, the Allow box and the toasts become Jocasta's own windows

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-11-02

## Question
While the close question of H3's first fault waited, the whole companion stood still. The close question runs on the service's own thread; his Allow box runs on its own thread so that "the service keeps running while it is open". Change the close question?

## Facts
Verified 2026-11-02 in the VM's benchmark sandbox, on 0517's build, and in the source.
- From 03:53Z to 04:06:43Z, while the close question waited, the companion answered nothing: `/health` timed out after 10 s, it used 0 ms of CPU in 10 s, and its refresh and log stopped, so 0431's restart never ran.
- The close question (0434, 0438) is Electrobun's `Utils.showMessageBox`, awaited on the app's main thread, which also runs the service (`src/app/main.ts:137`, `:154`).
- His Allow box (0484) is a `MessageBoxW` on a worker's thread, so the service keeps running (`src/app/confirm-worker.ts`). It is system-modal so that it stays in front while he works in another app (R1, FinalCheck).
- The urgent and blocked toasts (0407) are Electrobun's `showNotification`, a `Shell_NotifyIcon` balloon. In the VM none showed in any window state (`docs/testing.md` row 91, fail), and clicking one does nothing.
- OS boxes can't be styled. Electrobun 2.0.2's `BrowserWindow` has `setAlwaysOnTop`, `center`, `showInactive` and frameless, transparent windows; the dictation capsule (0409) is such a window showing a view of the page.

## Options
| Option | Pros | Cons |
|---|---|---|
| Own thread, like Allow | Nothing stalls behind an open question | In the next build; a second box kind on that thread |
| Leave it for after v1 | No change now | Closing the terminal by mistake while away stalls every thread's Jocasta tools until he answers |

Asked next, which pop-ups become Jocasta's own windows (any of): the close question; the Allow/Deny box; the Windows toasts; the startup error, which needs its own bundled page because no service runs then.

## Recommendation
Own thread, like Allow. Then: the close question, the Allow/Deny box and the toasts; the startup error stays the OS box.

## Choice
- The close question, his Allow/Deny box and the urgent and blocked toasts while the window is hidden or behind become Jocasta's own windows drawn by Electrobun: frameless, in the companion's look, each showing a view of the page. The service runs on while one is open.
- The close question and the Allow box show centred on the display and stay on top, as today's system-modal box does. The close question keeps 0438's texts, with Keep as the default and on Escape. The Allow box keeps its texts, with Deny as the default and on Escape, and no answer in 2 minutes is Deny. The service takes the Allow box's answer only with that window's one-time code, which the page never sees.
- A toast shows at the bottom-right corner of the display, on top and without taking the keyboard, for as long as the page keeps that toast; clicking it focuses that pane in herdr, as 0407 meant. It replaces Electrobun's balloon, so Windows' notification centre no longer keeps them.
- The startup error ("The Jocasta service didn't start.") stays the OS box: no page is served then.

## Reason
"yes everything electrobun native so it can be more stylish"; then the close question, the Allow/Deny box and the Windows toasts.

## Consequences
- Amends 0407 (the toasts), 0434 and 0438 (the close question's window) and 0484 (the Allow box's window); `src/app/confirm-worker.ts` goes.
- The Allow box's checks run again live: it is in front while another app has the keyboard, and its answer can't be forged from the page.
- H2's background pass can measure t3, the toast as he sees it with the window hidden (0520).
- Amended by 0524: the Allow window and the close question show on top without taking the keyboard, as the toasts do.
- Amended by 0552: no app; questions are answered as text in the `jocasta` pane and herdr's sidebar shows them waiting, with a Windows notification when the terminal isn't in front (a thread could answer through herdr's socket, an accepted risk).
- Amended by 0558: Allow takes a typed 3-letter code in herdr's popup; Esc denies.
