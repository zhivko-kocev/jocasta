# 0418 A more human voice: kokoro af_heart in the service

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-14

## Question
He asked for a more human-like voice. Which voice, and where does speech run?

## Facts
Verified 2026-10-14.
- Today: kokoro-js 1.2.1, voice `bm_george` (kokoro's VOICES.md grade C), whole messages synthesized in the companion page, which shares the main thread and GPU with whiteboard's canvas.
- kokoro grades: `af_heart` A, `af_bella` A-; the best male voices (`am_michael`, `am_fenrir`, `am_puck`) C+.
- Measured on his CPU (Core Ultra 7 155U) with sherpa-onnx, which Jarvis already ships for dictation: kokoro fp32 about 2.5–3.5 s for a 6–7 s sentence; int8 slower (7.8–8.7 s).
- PocketTTS clones a voice with first audio after 0.7–1.8 s, but its ready export is marked non-commercial; OpenAI `gpt-4o-mini-tts` is about $0.015 a minute and sends every line out.

## Options
| Option | Pros | Cons |
|---|---|---|
| Kokoro af_heart in the service, sentence-streamed | Best kokoro voice; local and free; speech leaves the page (smoother canvas); starts after the first sentence | Not a British male voice |
| Kokoro am_michael, same move | Male | Less natural (C+) |
| PocketTTS with a cloned voice | Most human locally | Non-commercial export, a licensed clip, a new dependency |
| Cloud gpt-4o-mini-tts | Most natural | Privacy, cost, network |

## Recommendation
Kokoro af_heart in the service, sentence-streamed.

## Choice
The voice is kokoro `af_heart`. Speech is synthesized in the Jarvis service (not the page), one sentence at a time, each sentence played as soon as it is ready; the page only plays audio.

## Reason
"Kokoro af_heart, in the service, sentence-streamed".

## Consequences
- Amends 0293, 0352 and 0373: the voice and where it runs.
- Amended by 0436: muting drops the voice from the companion's memory; unmuting loads and warms it again.
- Amended by 0552: spoken replies and the voice model go.
