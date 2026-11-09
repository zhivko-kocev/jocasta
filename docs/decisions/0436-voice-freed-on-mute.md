# 0436 The voice leaves memory when muted

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-15

## Question
The kokoro voice and the Parakeet recognizer stay loaded in the companion once used. Does muting free the voice?

## Facts
Verified 2026-10-15.
- The kokoro model (`model.onnx` 310.5 MB + `voices.bin` 26.9 MB, fp32) and the Parakeet encoder (int8, 622 MB + 17.4 MB) are never freed once loaded (`voice-tts.ts`, `dictation/server.ts`).
- [INFERENCE] +0.35–0.45 GB after the voice is unmuted once, +0.65–0.75 GB after the first dictation.
- Unmuting already loads and warms the voice (0418); Right Ctrl dictation is expected to answer at once.

## Options
| Option | Pros | Cons |
|---|---|---|
| Free the voice on mute | Memory back while he works muted; unmute already warms | A short load after unmute |
| Free both after idle | Most memory back | Right Ctrl waits for a load |
| Keep both | Instant everything | Up to ~1.2 GB held |

## Recommendation
Free the voice on mute.

## Choice
Muting drops the kokoro voice from the companion's memory; unmuting loads and warms it again (as unmute already does). Dictation's model stays loaded so Right Ctrl stays instant.

## Reason
"Free the voice on mute".

## Consequences
- Amends 0418 (the voice loaded in the service).
- Amended by 0552: the voice model goes, so there is nothing to free.
