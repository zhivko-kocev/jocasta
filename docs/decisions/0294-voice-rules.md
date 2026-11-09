# 0294 Voice rules

- Status: decided
- Kind: product
- Phase/slice: 7/7.4
- Date: 2026-10-08

## Question
Who speaks, when and how? (rules start from the omp plan's B4, G3 and G5, 0028)

## Facts
- Toasts with a sound announce blocked and finished threads (0271). The boss relays questions (0259).

## Options
| Option | Pros | Cons |
|---|---|---|
| Only the boss; reports and dictated answers; about 30 words; no code, paths or tables aloud; a HUD mute; speech replaces toast sounds | Calm | — |
| Speak everything the boss says | — | Noisy |
| Reports only | Quietest | No spoken answers |

## Recommendation
As described.

## Choice
Only the boss speaks; tabs are silent. It speaks reports of blocked and finished threads, and a one-sentence answer when he dictated the question. It leads with the sentence that matters, keeps to about 30 words, and never reads code, paths or tables aloud; it says where they are. A mute toggle sits on the HUD. While voice is on, speech replaces the toast sound (0271).

## Reason
Not given.

## Consequences
- Preview: "api's export is blocked: it needs you to pick v1 or v2. The question is in the boss chat."
- Amended by 0373: every reply to his message, typed or dictated, is read aloud sentence by sentence as it streams; code, paths and tables are still skipped.
- Amended by 0552: nothing is spoken any more; dictation stays.
