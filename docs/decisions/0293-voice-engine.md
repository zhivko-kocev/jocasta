# 0293 Voice engine

- Status: decided
- Kind: product
- Phase/slice: 7/7.4
- Date: 2026-10-08

## Question
Which engine speaks with a British voice?

## Facts
- Checked 2026-10-08: Windows voices here are US only (David, Zira, Mark); kokoro-js 1.2.1 is Apache-2.0 and runs Kokoro-82M in a web page (npm); Kokoro's British male voices bm_george and bm_fable are graded B/C (VOICES.md). The model downloads once from Hugging Face.
- Edge's "Online (Natural)" en-GB voices send text to Microsoft's cloud (not probed).

## Options
| Option | Pros | Cons |
|---|---|---|
| Kokoro in Frontier's Edge page, `bm_george` | Local; no host install | One model download |
| Edge's natural voices | Best quality, no setup | Text goes to Microsoft |
| Windows SAPI with an en-GB language pack | Built in | An install; robotic |

## Recommendation
Kokoro in Frontier's page, bm_george.

## Choice
kokoro-js in Frontier's page with the `bm_george` voice; the model is fetched once on first use and cached.

## Reason
Not given.

## Consequences
- The first model download asks him first, like any install.
- Amended by 0418: kokoro `af_heart`, synthesized in the service sentence by sentence.
- Amended by 0552: no companion app; spoken replies and the voice model go, dictation stays.
