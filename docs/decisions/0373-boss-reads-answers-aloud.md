# 0373 The boss reads his answers aloud

- Status: decided
- Kind: product
- Phase/slice: 7/7.4
- Vision: V2, V11
- Date: 2026-10-10

## Question
When does the boss speak, now that he finds the voice late or silent?

## Facts
- 0294: only a dictated question gets a spoken answer: its first sentence, after the boss's whole run has settled. Typed messages are never spoken.
- The page synthesized each line whole with kokoro-js and played it before it began the next; at most 3 lines waited, older ones were dropped.
- Measured on his machine (0354): WebGPU fp32 takes 3.2–3.8 s for a ~7-second sentence, after the first.
- His words on 2026-10-10: "the jarvis voice is late or sometimes doesnt even work".

## Options
| Option | Pros | Cons |
|---|---|---|
| Dictated only, sooner: the first sentence once it is complete, the voice warmed, stuck speech fixed | Keeps 0294's calm | Typed answers stay silent |
| Every answer's first sentence, typed too | Jarvis answers aloud more often | Still one sentence |
| Full answers, sentence by sentence as they stream, dictated or typed | Most JARVIS-like | Most talk |

## Recommendation
Dictated only, sooner.

## Choice
- Every reply to his message, typed or dictated, is read aloud sentence by sentence as it streams, until the boss's run ends.
- Code blocks, tables, headings, and sentences with code or a path are skipped; when anything was skipped, the answer ends with "The details are in the boss chat." Markdown marks and list bullets aren't read; a link is read by its text.
- Turns he didn't start stay as they were: blocked and finished reports, urgent events and the `speak` tool (0294, 0296, 0299).
- He cuts it short by sending a message, dictating or muting. The mute and quiet hours silence it as before.
- The page synthesizes the next sentence while one plays, warms the voice when it loads, and lets go of a line that hasn't finished 2 s after its length. "Not now" on the download question holds until he turns the voice off and on.
- The sentence being said is marked in the chat (0369).

## Reason
"Speak full answers": "Read whole answers aloud (sentence by sentence, streamed as they arrive), dictated or typed. Most JARVIS-like, most talk; code, paths and tables still skipped."

## Consequences
- Amends 0294 (a one-sentence answer to dictated questions only) and 0368 (push-to-talk's "one-sentence answer"). The `speak` tool (0296) keeps its 30 words, for turns he didn't start.
- `boss.send` no longer carries `dictated`; `speak` carries `shown`, the words as the chat shows them.
- Differs from the recommendation.
- The closing line, the interruptions, the timings and "not now" holding were chosen by the builders under 0363.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0405: events get their own short spoken line; answers are spoken per completed message.
- Amended by 0418: speech runs in the service, sentence-streamed.
- Amended by 0552: answers are no longer read aloud.
