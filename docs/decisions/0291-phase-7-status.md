# 0291 Phase 7 slice status

- Status: decided
- Kind: product
- Phase/slice: 7/opening
- Date: 2026-10-08

## Question
Which phase 7 slices are in v1?

## Facts
- V6 asks for animations and diagrams "like devdotfast/whiteboard"; V2 is about input, so voice output is outside the vision. Frontier's canvas column exists from 4.3 (0257), which settles 7.1.
- Checked 2026-10-08: the only Windows voices on this machine are US (David, Zira, Mark). Kokoro-82M (Apache-2.0) has 4 British male voices (https://huggingface.co/hexgrad/Kokoro-82M/raw/main/VOICES.md).
- The recommendation kept 7.2 and only built-in activity indicators for 7.3, and put 7.4 after v1; he ticked all three.

## Options
| Option | Pros | Cons |
|---|---|---|
| 7.2 diagrams | Vision V6 | — |
| 7.3 animations beyond spinners | Vision V6 | More UI work |
| 7.4 voice output | The film's British voice | A TTS engine; not in the vision |

## Recommendation
7.2 v1; 7.3 as built-in indicators only; 7.4 after v1.

## Choice
7.1 is Frontier's canvas (0257); 7.2, 7.3 and 7.4 are v1.

## Reason
Not given.

## Consequences
- Differs from the recommendation (7.3 and 7.4 in v1).
