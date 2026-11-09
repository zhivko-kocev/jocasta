# 0354 Phase 7 as-built and the voice's speed

- Status: decided
- Kind: product
- Phase/slice: 7/7.1–7.4
- Date: 2026-10-09

## Question
Are phase 7's built choices accepted, and how is the voice made faster?

## Facts
- Measured 2026-10-09 in headless Edge: kokoro-js (q8, wasm, one thread, since the page isn't cross-origin isolated) took 17.5 seconds to synthesize a 5-second sentence. The model came from huggingface.co and the onnxruntime wasm (21.6 MB) from cdn.jsdelivr.net, cached by the browser.
- WebGPU runs it on the GPU where available. Threaded wasm needs the page to be cross-origin isolated (COOP/COEP headers), which blocks cross-origin loads unless they opt in, so the voice files would be served by Jarvis. Speedups for both are [INFERENCE], measured at build.
- He asked what threaded CPU means; it was explained.

## Options
| Option | Pros | Cons |
|---|---|---|
| WebGPU first, threaded CPU fallback | Fastest; a better fallback | Jarvis serves the voice files; an isolated page |
| WebGPU first, plain CPU fallback | Less work | Slow fallback |
| CPU as built | Simple | 17.5 seconds for a sentence |

## Recommendation
WebGPU first, CPU fallback.

## Choice
- The voice runs on WebGPU first, with threaded wasm as the fallback: Frontier serves the page with COOP/COEP, and Jarvis downloads the voice runtime and model once into its data dir and serves them itself.
- The model is chosen by device: a machine with WebGPU downloads the full-precision model (about 325 MB), others the q8 model (about 92 MB). Measured at build on his machine (Intel Xe, busy), for a ~7-second sentence after the first: WebGPU fp32 3.2–3.8 s; WebGPU fp16 7.9 s; WebGPU q8 about 12 s; 4-thread wasm q8 13–21 s. He chose by device over fp16 on WebGPU and q8 everywhere.
- Accepted as built: the `draw(drawing?, title?, ops[])` tool with add_block, add, update and remove by id; markdown, code_peek, sequence and flow_diagram blocks; ids `b1`, `a1`, `s1`, `n1`, `e1` and drawings `d-` plus 8 hex; the boss doesn't draw. Scratchpads in `<data>/projects/<pid>/drawings/<id>.json` and `<data>/home/drawings/`; a lone harness saves the same way. The code peek opens inside the canvas. The speech texts for blocked and finished threads, and `speak` refusing code, paths, tables and texts over 40 words. The voice download card ("The boss wants to speak. Its voice model (about 90 MB) downloads once…", download / not now), HUD `voice on|off` and `canvas`. The canvas at 40% width sliding in, the github-dark-dimmed theme with 26 grammars, dashed amber decisions and pill terminals, 160 ms per part, the tour with ← → Esc, markdown images as links and raw HTML as text. `draw` and `speak` in chat-only threads and plan mode.

## Reason
"webgpu + threaded cpu - what doess threaded cpu mean"; then confirmed.

## Consequences
- Appendix I: the voice files under the data dir, and COOP/COEP on Frontier's page.
- Differs from the recommendation, which kept the plain CPU fallback.
- Amended by 0369 and 0371: the code theme follows the look, and a part draws in every 220 ms over 640 ms.
- Amended by 0372: the boss draws, on his own scratchpad or a project's.
- Amended by 0393: whiteboard's canvas, built from source, replaces Jarvis's canvas and `draw`.
- Amended by 0552: no app; the voice files and the page's policy go with it.
