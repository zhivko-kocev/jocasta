# 0367 Frontier becomes an Electrobun desktop app

- Status: decided
- Kind: product
- Phase/slice: 4/4.1, 4.8, 4.10
- Date: 2026-10-09

## Question
He asked how Whiteboard ships its UI and whether Tauri or Electron would be better than opening Frontier in a browser's app mode. What does Frontier ship as?

## Facts
Checked 2026-10-09.
- Whiteboard vendors Code-OSS, whose `package.json` pins `electron` 43.7.9, so it bundles its own Chromium.
- Electron 44.7.0 (MIT) draws exactly as Chrome does; the cost is about 150 MB per release, more memory and about a second of startup.
- Tauri 2 (`@tauri-apps/cli` 2.12.1) needs a Rust toolchain; it uses WebView2 on Windows and WebKitGTK on Linux, which has no WebGPU.
- Electrobun 2.0.2 (MIT, https://github.com/blackboardsh/electrobun): TypeScript desktop apps, built with its Hutch CLI. Its main process runs on its own Cottontail runtime or on Bun. It uses the system webview by default, or a pinned Chromium with `bundleCEF`, and has tray and updater APIs. It is young, and its one maintainer writes that issues and PRs may go unanswered.
- Frontier today is Jarvis's Bun server, opened in Chrome's app mode (0251, 0366). The server runs the boss's Pi session, `bun:sqlite`, the sherpa-onnx dictation addon, the voice files and the pollers; thread harnesses run in the background host (0357).

## Options
| Option | Pros | Cons |
|---|---|---|
| Chrome app mode for v1, a shell after v1 | Nothing to redo now | No tray or global key in v1 |
| Electron | Mature; Chromium everywhere | About 150 MB more per release; Node main |
| Electrobun | TypeScript and Bun end to end; small with the system webview | Young; single maintainer; Hutch toolchain |
| Tauri | Smallest | Rust; no WebGPU on Linux |

Follow-ups: engine (bundled Chromium | system webview); shape (a thin window shell next to `jarvis.exe` | Frontier's server in Electrobun's main process); extras now (tray, global push-to-talk key, autostart).

## Recommendation
Chrome app mode for v1 and a shell after v1. Then bundled Chromium, a thin shell, and the extras after v1.

## Choice
- Frontier becomes an Electrobun desktop app now, before v1 is done. His words: "i think electrobun fits best or?"
- The system webview: WebView2 on Windows, WebKit on macOS and Linux.
- The full app: Frontier's server and the boss run in Electrobun's main process. Thread harnesses stay in the background host (0357), and the `jarvis` CLI stays its own executable. His words: "maybe full app fits better hm?", then "Full app anyway" after the costs.
- In v1 now: a tray icon; a global push-to-talk key that dictates to the boss while Frontier isn't focused; autostart at logon.

## Reason
His words, quoted above.

## Consequences
- Amends 0251 and 0366 (the window), 0253 (the app keeps running in the tray when its window closes), 0273 (a global push-to-talk key besides holding Space in the boss chat), and 4.10's status for autostart (0252).
- On Linux, WebKitGTK has no WebGPU, so the voice uses its wasm fallback there (0354).
- Build-time rule: a spike on this machine comes first. It runs Frontier's server in Electrobun's main process on Windows and checks: the boss's Pi session, `bun:sqlite`, the sherpa-onnx addon, the voice on WebGPU with cross-origin isolation, the microphone, notifications, xterm's WebGL, the tray, a held global key, and autostart. Anything that fails comes back to him with what was measured before the rest is built.
- Installs: Hutch and Electrobun at versions pinned when the spike starts.
- Differs from the recommendation on timing, engine and shape.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0434: the window's × closes Jarvis as one, asking first while threads are working or awaiting him; Keep hides it to the tray.
- Amended by 0549: autostart at logon stays as `frontier.autostart`, off by default; nothing keeps running in the tray once Jocasta closes.
- Amended by 0548: the companion window becomes the canvas; the rail moves into a herdr pane.
- Amended by 0552: no Electrobun app; the service is a plain Bun process started by `jocasta`.
- Amended by 0557: autostart and its Run value go.
