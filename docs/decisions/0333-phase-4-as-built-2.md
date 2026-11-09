# 0333 Phase 4 as-built, second pass

- Status: decided
- Kind: product
- Phase/slice: 4/4.1–4.12
- Date: 2026-10-08

## Question
Are the rest of phase 4's built choices accepted, and how are the gaps found at integration closed?

## Facts
- Appendix I promises `JARVIS=1` and `JARVIS_THREAD_ID` to every command a thread runs. Frontier's threads share one process, so the env can't be set per thread; Pi 1.1.0's bash and powershell tools take a `spawnHook` that sets env per command, and tools passed as `customTools` replace the built-ins by name and stay wrapped by the extensions (guard, redaction).
- Pi's folder-trust prompt is a terminal dialog; a hosted session in an untrusted folder opened without its project resources.
- React Compiler (0328) runs through Babel, so it needs `@babel/core`, which wasn't installed.
- `sherpa-onnx-node` 1.13.8 ships its native code per platform; only `sherpa-onnx-win-x64` was installed.
- No provider reports usage left through Pi 1.1.0, so 0275's figure and warning never show.

## Options
| Option | Pros | Cons |
|---|---|---|
| Accept the listed texts; per-command env through `spawnHook`; a trust card in the page; compiler on; the two other platform packages; digest on request; dormant usage warning | Keeps appendix I's promise and Pi's trust check in the page; all three builds carry dictation | Two more dev dependencies; dormant code |
| Accept the gaps as they are | Nothing to build | Thread id missing in Frontier; trust only from the terminal |

## Recommendation
The first row.

## Choice
- Accepted as built: the cookie `jarvis_frontier`, the WebSocket at `/ws` and `/health`; catalog.db v3's `thread_runtime` table (thread_id, project_id, state, boss_run, queued_text, updated_at); the boss's event texts; `That was already answered.` and `no open thread X.`; `jarvis: Frontier is at <url>` when no app-mode browser is found; Ctrl+C stops Frontier as Quit does; a run ends in the thread's `report` status, `cancelled` after a stop, `failed` when its last request errored, otherwise idle.
- Every command a Frontier thread runs through `bash` or `powershell` gets `JARVIS=1` and `JARVIS_THREAD_ID`, through Pi's `spawnHook`.
- A project with untrusted `.pi/` resources shows Pi's trust question as a card in its tab and the boss chat; the answer goes into Pi's own trust store.
- The digest shows on request and at start-up only.
- The usage-left figure and its 80% warning stay built and dormant until a provider reports usage left.
- React Compiler is on: `@babel/core` (dev, exact) is installed and a Bun build plugin runs the compiler on the page's `.tsx` files.
- `sherpa-onnx-linux-x64` and `sherpa-onnx-darwin-arm64` 1.13.8 are dev dependencies, so the Linux and macOS builds carry dictation.

## Reason
"explain more on unticked"; then the trust card, digest on request and the dormant warning were chosen from the options.

## Consequences
- Appendix I: catalog.db v3, `<data>/frontier.json`, and the hosted-thread env.
- Bun skips platform packages whose `os` doesn't match the machine, so a fresh `bun install` on Windows doesn't extract the Linux and macOS ones; `scripts/build.ts` prints a warning and packages the build without dictation when one is missing.
- `!` commands typed into a tab don't go through the tool hook; the page has no `!` input.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
