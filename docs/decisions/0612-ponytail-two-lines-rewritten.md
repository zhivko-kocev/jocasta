# 0612 Ponytail: two lines rewritten, one removed

- Status: v1
- Kind: product, prompts
- Phase/slice: program
- Date: 2026-11-14

## Question
Ponytail is appended to every project and thread pane (0427). Two of its lines pull against the standard (0609) and against how the repo is written, and one is about another repo. Do they change?

## Facts
Verified 2026-11-14 (the code-quality section of `docs/research/state-of-the-art.md`).
- `config/ponytail.md` is Ponytail 5.2.0, verbatim from its npm package (MIT), plus Jocasta's "Running tests" section (0469). It is 1,612 tokens in every project and thread pane.
- "Comment only the why the code cannot show, in one line." The standard asks for a comment on each exported item, written for its caller. The repo follows the standard: 96.8% of its functions carry one.
- "The shortest working diff wins, once you know everything it must touch." The standard asks for the structure to be fixed when the task shows it is wrong.
- "(Yes, this file also applies to agents working on the ponytail repo itself. Especially to them.)" is about Ponytail's own repo.
- The other lines either agree with the standard (reuse first, the root cause, error handling kept when code moves, nothing unasked) or are cost rules the standard has no view on.

## Options
| Option | Pros | Cons |
|---|---|---|
| Rewrite the two, remove the one | The always-on text stops contradicting the standard; size about equal | Jocasta's copy then differs from upstream's text; softening "shortest diff" may bring larger diffs |
| Only the comment line | The clearest conflict goes | The diff rule stays |
| Leave Ponytail as it is | Upstream's text, unchanged | Two lines keep pulling against the standard |

## Recommendation
Rewrite the two, remove the one.

## Choice
- "Comment only the why the code cannot show, in one line." becomes: "Comment each exported function, class and type for its caller: what it does, units, edge cases, errors. Inside a function, comment only the why the code cannot show."
- "The shortest working diff wins, once you know everything it must touch. A one-liner that needs decoding is not short." and "Keep the structure the codebase already has: its layers, interfaces and conventions." become: "Add nothing the task does not need. Keep the structure the codebase has; when the task shows that structure is wrong, fix the structure, not a special case around it. A one-liner that needs decoding is not short."
- "(Yes, this file also applies to agents working on the ponytail repo itself. Especially to them.)" is removed.

## Reason
His answer: "rewrite".

## Consequences
- Amends 0427: the file is no longer upstream's text verbatim, and its header says which lines Jocasta changed.
- The benchmark's `ponytail` variant and `f-ponytail-if` read the same text, so they measure the rewritten one.
- Diff size per thread is in the scorecard (0610), to see whether diffs grow.
