# 0355 Phase 7 as-built, the voice files

- Status: decided
- Kind: product
- Phase/slice: 7/7.4
- Date: 2026-10-09

## Question
Are the voice-file choices as built accepted?

## Facts
- Measured at build: WebGPU fp32 speaks a ~7-second sentence in 3.2–4.8 seconds after the first; the first takes 9–17 seconds while the model loads.
- In the compiled exe probe on the WebGPU path, the page made no remote requests.

## Options
| Option | Pros | Cons |
|---|---|---|
| Accept as built | Done | — |
| Rework | — | — |

## Recommendation
Accept as built.

## Choice
- Voice files live in `<data>/voice/` (models and runtime), served at `/voice/`; downloaded only after his click, each checked by size, a half download thrown away; about 330 MB on a WebGPU machine, about 90 MB otherwise.
- The card reads "The boss wants to speak. Its voice model (about 330 MB) downloads once and stays in Jarvis's data folder. Download it?" with download and not now.
- The model comes from `onnx-community/Kokoro-82M-v1.0-ONNX` at a pinned Hugging Face revision and the runtime wasm from jsDelivr (`@huggingface/transformers` 3.8.1), both downloaded by Jarvis, never by the page.
- Frontier's page is cross-origin isolated and loads nothing from other sites.

## Reason
All ticked.

## Consequences
- Appendix I: `<data>/voice/`, `/voice/`, COOP/COEP.
