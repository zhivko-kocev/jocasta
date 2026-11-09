# 0552 No companion app: the service is a plain Bun process, diagrams open in the browser, dictation stays without spoken replies, and questions are answered in herdr

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V2, V3, V6
- Date: 2026-11-09

## Question
After 0548 he asked: "isnt the companion in the herdr why do we need anything ?" What remains outside herdr?

## Facts
Verified 2026-11-08.
- The companion (Electrobun 2.0.2, WebView2) holds the service, the canvas (whiteboard, V6), the Allow, close and toast windows (0519, 0524), the push-to-talk capsule, microphone capture (`getUserMedia`, `src/frontier/dictation/capture.ts`) and speech playback (`AudioContext`, `src/frontier/ui/voice.ts`); its own UI uses React 19 with the React Compiler.
- The push-to-talk hook is a Bun worker calling Win32 directly (`src/app/ptt-worker.ts`); Bun has no built-in microphone or speaker access, so recording without a webview needs a native audio helper [INFERENCE: not yet checked on Windows].
- V2: "He can dictate and Frontier listens, or he can chat with it."
- herdr's socket takes `pane.send_text`, `agent.prompt` and `pane.read` from any process of his (0484, F2); the boss key keeps the boss's tools to the boss pane. Threads run as him, and the guard's known bypasses are an accepted risk (0485).
- whiteboard's server serves the canvas over HTTP; whiteboard's canvas bundle carries its own React.

## Options
| Item | Options (pros / cons) |
|---|---|
| Outside herdr | No permanent window (canvas, questions and voice kept) · No app at all (lightest; voice and the trusted window go) |
| Voice | Keep voice through a helper · Dictation only · Drop voice, changing V2 |
| Questions | Native Windows dialogs (as safe as today) · A small webview per question (our look, a dependency) · Text in herdr (a thread could answer through herdr's socket) |

## Recommendation
No permanent window; keep voice; native Windows dialogs.

## Choice
- No Electrobun app. `jocasta` starts the service as a plain Bun process beside herdr's session; the push-to-talk worker moves into it.
- Diagrams and reviews open in his browser from whiteboard's server.
- Dictation stays (V2): Right Ctrl records through a native audio helper, chosen after a Windows spike (a new dependency, asked then), and the text goes to the boss. Spoken replies and the voice model go.
- Allow and close questions appear as text in the `jocasta` pane (0551) and in herdr's sidebar as waiting, answered there; a Windows notification pings him when the terminal isn't in front.
- React, React DOM, their types, the React Compiler and Babel leave the repo; Monaco and the WebView2 windows go with the app.

## Reason
"No app at all"; "Dictation only"; on the questions "maybe just appear as text in herdr and i answer them?", then, shown that a thread could answer one through herdr's socket: "try with the recommended and we see how it behaves, i dont get how a thread can answer itself".

## Consequences
- Differs from the recommendation on all three.
- Accepted risk, beside 0485's: a thread, or text injected into what it reads, can answer a question through herdr's socket (`herdr pane send-text <pane> "y"`). The boss key (0484) stays.
- Amends 0367, 0368 and 0392 (the Electrobun app and companion), 0393 (whiteboard's canvas in the companion), 0293, 0294, 0296, 0354, 0373, 0418 and 0436 (spoken replies and the voice model), 0329 and 0407 (in-app toasts), 0519 and 0524 (Jocasta's own question and toast windows), 0544 (the CLI's Allow window and the companion's buttons), 0548 (the canvas window) and 0549 (the app's lifecycle). Dictation's model stays (0330).
- H2 (time to the toast), H5 and H6 measure the new shape when every benchmark runs again (0533); H2's end point is decided with the build.
- A spike for the audio helper joins the four of 0536 and 0542.
- Amended by 0553: questions open in a herdr popup instead of the `jocasta` pane (0558 found herdr's socket keys reach the popup, so Allow takes a typed code).
- Amended by 0556: no audio helper; dictation is omp's own speech-to-text in the boss pane, toggled by Right Ctrl through herdr.
- Amended by 0563: the close question appears in a terminal reopened on the same herdr session when he closes it while threads work.
