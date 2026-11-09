# 0272 Dictation engine

- Status: decided
- Kind: product
- Phase/slice: 4/4.8
- Date: 2026-10-08

## Question
Which engine turns his speech into text?

## Facts
- The web UI records the mic through the browser (getUserMedia). Edge's built-in speech recognition most likely sends audio to Microsoft's cloud ([INFERENCE], not verified on today's Edge build); client names and code he dictates would leave the PC.
- Candidates: whisper.cpp's server; a sherpa-onnx sidecar running Parakeet TDT 0.6B v3 (the omp plan's engine).
- His question: "is the recomendation better than local only cause local only seems good enough?"; the answer changed the recommendation to local only.

## Options
| Option | Pros | Cons |
|---|---|---|
| A spike over Edge's speech, whisper.cpp and Parakeet | Zero install if Edge wins | Audio may leave the PC |
| Local only: a spike over whisper.cpp and Parakeet | Audio stays on the PC; offline | One install |
| Parakeet now | No spike | Unmeasured |

## Recommendation
Local only, after his question (the first recommendation included Edge's cloud speech).

## Choice
Now (0330): Parakeet without the spike.

Originally: Local only. A spike on 20 of his dictated sentences compares Parakeet TDT 0.6B v3 (sherpa-onnx) and whisper.cpp: word error rate and seconds to text. Rule: the smaller setup within 2 points of the best word error rate wins; a result the rule doesn't settle reopens the decision. Installing either asks him first.

## Reason
"is the recomendation better than local only cause local only seems good enough?"

## Consequences
- The sidecar's container question (0.7) is settled by the winner's setup.
- Amended by 0330: Parakeet chosen; no spike.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
