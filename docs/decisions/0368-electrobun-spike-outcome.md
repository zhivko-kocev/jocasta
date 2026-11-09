# 0368 Electrobun after the spike, and the global push-to-talk key

- Status: decided
- Kind: product
- Phase/slice: 4/4.1, 4.8
- Date: 2026-10-09

## Question
0367's spike found blockers. Does Frontier still become an Electrobun app, and how does the global push-to-talk key behave?

## Facts
Measured on his machine on 2026-10-09 (Electrobun 2.0.2, Hutch 0.27.1, WebView2 154):
- Jarvis's Frontier server runs unmodified in Electrobun's Bun main process: the boss's Pi session, `bun:sqlite` with FTS5, and the sherpa-onnx addon. In WebView2, WebGPU (Intel Xe-LPG), WebGL2 with xterm's WebGL addon, the microphone and notifications work. The tray, a held global key and autostart work.
- Electrobun hard-codes `--disable-web-security` for WebView2 in its native wrapper, with no setting to remove it. That turns off cross-origin isolation, so the voice's threaded wasm fallback can't run, and it turns off the browser's same-origin and CORS protection for Frontier's page.
- Its bundled Chromium (`bundleCEF`) crashed the GPU process on this machine (no WebGPU, no WebGL) and adds about 700 MB.
- It runs its own Bun 1.4.0, not Jarvis's pinned 1.4.2, and ships as a folder app with a setup exe, about 35 MB.
- The global key worked through `bun:ffi` (a low-level keyboard hook, about 0.2 ms); autostart through an HKCU Run value; neither needs Electrobun. Electrobun has no single-instance or autostart API.

## Options
| Option | Pros | Cons |
|---|---|---|
| No Electrobun; Jarvis adds tray, global key and autostart itself | Chrome app mode keeps isolation and web security; one exe; pinned Bun | No Electrobun window |
| Electrobun with WebView2 anyway | The app he chose | Web security and isolation off; Bun 1.4.0; folder app; Hutch |
| Fork or patch Electrobun's native wrapper | Isolation back | Jarvis maintains a fork |

Global key: Right Ctrl | Right Alt | Caps Lock | F13–F24. On release: sent to the boss at once with a spoken answer | typed into the boss input with Frontier brought forward.

## Recommendation
No Electrobun; Right Ctrl; sent at once with a spoken answer.

## Choice
- Electrobun with WebView2 anyway, accepting that web security and cross-origin isolation are off in its window; the voice runs on WebGPU only.
- The global push-to-talk key is Right Ctrl, held alone, anywhere in Windows. On release, what he said is sent to the boss at once, and its one-sentence answer is spoken (0294); Frontier stays where it is.

## Reason
Not given.

## Consequences
- Amends 0367 (the spike's rule is settled), 0354 (no threaded wasm fallback in the app) and 0273 (the global key sends at once; holding Space in the boss chat still types into the input).
- Because web security is off, Frontier's page gets a strict Content-Security-Policy (only its own origin) and never loads remote content.
- Electrobun's Bun 1.4.0 runs the app; `jarvis.exe` and the host stay on the pinned Bun 1.4.2.
- Differs from the recommendation on Electrobun.
- Amended by 0373: push-to-talk's answer is read aloud in full, like every reply.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0493: the page's policy allows `'unsafe-eval'` for whiteboard's software maps (libavoid).
- Amended by 0552: no Electrobun app; push-to-talk's hook moves into the Bun service and records through a native audio helper.
- Amended by 0556: the hook sends omp's `app.stt.toggle` key to the boss pane on press and release; omp records and transcribes.
