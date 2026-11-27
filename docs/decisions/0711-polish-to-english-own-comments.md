# 0711 Polish, To English, his own comments

- Status: v1 (built 2026-11-27: the service, the cards' Polish and To English, the window's Add comment box and summary box)
- Kind: product, prompts
- Phase/slice: ultron 0.3 iteration, M2 R6
- Date: 2026-11-26
- Amends: 0680 (Reword goes; Polish and To English take its place on the `polish` role), 0696 (the snapshot gains `modelRoles.polish`), 0721 (the cards' controls)

## Question
Reword asks a cloud helper to rewrite a comment as he instructs: it can change what the comment says, and his own words have nowhere to go but an existing card. How does a review become his, in his words, with help only where he wants it?

## Facts
- Reword runs `omp -p --model @smol` with an instruction, the note, its place and its quote; the answer is only a suggestion until he accepts it (an `edit`).
- omp 19.2.1 resolves a custom role `@polish` once `modelRoles.polish` is set, first available model wins: checked on 2026-11-27 with omp's agent folder copied into a scratch home (his settings untouched). Without the role omp exits 1 with `Model "@polish" not found`.
- He writes in Macedonian and in English; reviews go out in English.

## Options
| Option | Pros | Cons |
|---|---|---|
| Polish (grammar and phrasing only) and To English on a local-first role, input his text only; his own cards and a summary box | His meaning stays his; nothing but his words leaves; the cheap local model does it when it can | Two fixed prompts instead of free instructions |
| Keep Reword | Built | A free instruction can change the claims; the note's code goes out with it |

## Recommendation
Polish and To English on `@polish`, his own cards and a summary.

## Choice
His answers of 2026-11-26 and the approved plan:
- `reword` is removed everywhere. Card actions `polish {card, text}` and `english {card, text}` run `omp -p --model @polish --no-tools --no-session --no-extensions --no-skills --no-rules --system-prompt <prompt>` with input his text only (sealed as model text, 0681), 60 s; the answer is a suggestion `{card, text}`, stored only when he accepts it (an `edit`, or a `summary` for the summary box).
- Prompts, verbatim: Polish `Fix the grammar, spelling and phrasing of this review comment. Keep its language, its meaning, every technical claim and every identifier exactly. Do not add or remove points. Answer with the comment text only.`; To English `Translate this review comment into clear English. Keep every technical claim and identifier exactly. Do not add or remove points. Answer with the comment text only.`
- The snapshot (`config/omp/config.yml`) gains `modelRoles.polish: ollama/qwen3:4b, anthropic/claude-haiku-5-5:low`. `@polish` resolves on omp 19.2.1, so the plan's `@commit` contingency isn't used.
- No model for the role: the card says `Polish isn't available: no model answers for the polish role.` (To English: `To English isn't available: no model answers for the polish role.`).
- New action `add {text, file?, line?}`: a card of his own, kind `yours` (shown "yours"), no proof, verdict `post`, P2. With a file it needs a line, and the head must have that file with that line; the card quotes that line. Without one it is a comment on the pull request as a whole.
- New action `summary {text}`: the review's summary box, kept in `review.json`; an empty text clears it.
- Built details: Polish and To English work on what is in the card's editor (or the summary box) when pressed, unsaved words included; `card: "summary"` names the summary box. His cards and the summary are on pull-request pages, whose cards he edits; a thread page's cards stay the reviewer's, and his words for a thread go through Send back.
- Built details, the window (2026-11-27): under the cards of a pull-request page that isn't closed, `Add your own comment` is a text box, a one-line `File (optional), as in the diff` box, a `Line` box that keeps digits only, and Add (blue; quiet while the text is empty). Add sends `add` with the file and the line only when given (the service checks them and says why it refuses under the box); the answer's new card appears with its own editor, the box empties, and his unsaved words in other cards stay. The verdict section (`Your review`) holds the summary box with Save (when it differs from the saved summary), Polish and To English; a summary suggestion's Accept sends `summary`.

## Reason
His review says what he means, in his words; the helper fixes language and nothing else, on a local model first.

## Consequences
- `pr_actions.rs`: `polish`, `english`, `add`, `summary`; `REWORD_SYSTEM` and `reword` go; `POLISH_SYSTEM`, `ENGLISH_SYSTEM`.
- `model.rs`: `Kind::Yours`; `review.json` gains `summary`.
- `view/`: Polish and To English beside Save, the suggestion box with Accept / Dismiss, an Add comment box.
- `config/omp/config.yml`: `modelRoles.polish`.
