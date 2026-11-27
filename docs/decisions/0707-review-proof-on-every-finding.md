# 0707 Review: proof on every finding

- Status: v1 (built 2026-11-27)
- Kind: product, prompts
- Phase/slice: ultron 0.3 iteration, M2 R3
- Date: 2026-11-26
- Amends: 0678 (a note has a kind; a finding carries a proof Jocasta ran), 0680 (questions and opinions aren't published until he switches them), 0611 (design thoughts go in as opinions)

## Question
A review note today is checked only for its quote: the code is where the note says, but the claim about it is the model's word. His measurements found every blocker note acted on and none of the nits. How does a note earn its place on the page?

## Facts
- `steps.rs` (0706) runs a `{run, expect}` step where the review runs: in the clean room for a pull request (on his machine when he allowed it), in the worktree under the guard for his own threads; `Card.ran` already keeps the step that proves a card, and the page already shows it behind a disclosure.
- The thread reviewer is told to hand in at most three design findings (0611); only the prompt holds that limit.
- A note's `verdict` (`post`, `question`, `skip`) decides whether Publish is offered; nothing on the page changes it.

## Options
| Option | Pros | Cons |
|---|---|---|
| Three kinds: a finding needs a proof Jocasta re-runs; questions and opinions (at most three) need none and stay his until he switches them | Every published claim is reproducible; unprovable taste is capped and never reaches the author unasked | A reviewer must write a command for each finding |
| Keep the quote check only | Nothing to build | Claims stay unproven; nits keep costing his time |

## Recommendation
Three kinds, proof on every finding.

## Choice
His answers of 2026-11-26 and the approved plan:
- `review_note` gains `kind: finding | question | opinion` (required). A `finding` needs `proof: {run, expect}`, run once by Jocasta through `steps.rs` exactly like a `review_try` step (same place, same guard, same judgement); it is kept only when the proof gives `expect`, and the card keeps the run as its proof.
- Refusals, exactly: a finding without a proof `review_note: a finding needs a proof (a command and the output it must give); hand it in as a question or an opinion otherwise`; a proof that doesn't give its expectation `review_note: the proof didn't reproduce: expected "<expect>", got exit <n>: <tail>`. A proof that refuses to reproduce doesn't end the review (only `review_try` does).
- At most 3 opinions per review: the fourth is refused (`review_note: this review has 3 opinions already, the most it keeps; hand in findings with proofs or questions`). The thread reviewer's design findings (0611) go in as opinions unless they are proven.
- Built details: a question or an opinion carries no proof (one given is refused: `review_note: only a finding carries a proof`), is P2 whatever severity is given (unproven thoughts never block) and is stored with verdict `skip`; the reviewer's `verdict` applies to findings only. `file`, `line` and `quote` stay required and checked for every kind.
- Questions and opinions are never published unless he switches the card's verdict: each pull-request card has a verdict switch (`post`, `question`, `skip`; the page action `verdict {card, verdict}`, refused once the card is published).
- The page shows each card's kind, and a proven finding's `✓ proven` with the command, the expectation, the exit and the output tail behind a disclosure.
- `config/pr-review.md` and the thread reviewer's prompt (`review_prompt.rs`) teach the kinds and the proof.

## Reason
What reaches the author is what Jocasta reproduced; taste is capped and stays his to send.

## Consequences
- `model.rs`: `Kind` and `Card.kind` (a card stored before this reads as a finding).
- `review_notes.rs` reads `kind` and `proof`; `tools.rs`' `review_note` runs the proof through `works.rs`' place and guard and caps opinions.
- `pr_actions.rs` gains the `verdict` action; `brief.js` the kind badge and the verdict switch.
- `tool_specs.rs` (and its fixture) describe `kind` and `proof`; `review_note` becomes a long-running tool.
