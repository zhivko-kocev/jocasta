# 0507 The agent writes the thesis and the slides at the end

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-01

## Question
0483 has him write every sentence of the thesis and the slides, because FINKI's template forbids AI writing. He says his mentor allows it. Who writes the thesis document and the slides, when, and how does the thesis say so?

## Facts
Verified 2026-11-01.
- FINKI's template, verbatim: "Користењето на вештачка интелигенција за пишување или значително помагање при пишувањето на дипломската работа е строго забрането." It names no exception for a mentor's permission (`D:/thesis/sources/finki-thesis.md` §3).
- UKIM's AI declaration (28.11.2024) forbids AI use "in a manner not permitted by the professor and assistant", and requires citing the AI tools that contributed, verifying their output and the author taking responsibility (same file, §3).
- His report, not verified: his mentor allows it. No written permission is in `D:/thesis` yet.
- `D:/thesis/thesis.docx` holds FINKI's template with the outline's headings and no text. The kit holds the outline, an evidence pack per chapter with each fact's source file and line, figures, charts, data, the bibliography, the outlook sources, the disclosure facts, the slide template and the recorded demo.

## Options
What the agent writes:

| Option | Pros | Cons |
|---|---|---|
| The thesis document only | What he asked for; the slides stay his | The slides and the text written apart |
| The thesis and the slides | One author for both, so the slides follow the text | Nothing of the defense in his words but his talk |

Who writes the disclosure appendix (Прилог Г):

| Option | Pros | Cons |
|---|---|---|
| The agent, and he checks it | Complete and plain, from the facts list | The statement about the text isn't in his words |
| He writes it from the facts list | The statement in his own words | One more piece for him |

## Recommendation
The thesis document only; the agent writes Прилог Г and he checks it.

## Choice
- Once everything else is finished (the benchmarks with A5, his review of v1 and the cutover), the agent writes `D:/thesis/thesis.docx` and the defense slides: in Macedonian, in FINKI's template, from the evidence packs, the data and the records. 0483's language and place stand.
- Прилог Г says plainly that an AI agent wrote the text and the slides under his direction, with his mentor's permission, and that he checked every fact and takes responsibility for it. The agent writes it; he checks it.
- Until then, the agent keeps preparing the kit as 0483 lists.

## Reason
"my mentor alows it and you fill the thesis document after we finish with everything"; then "The thesis and the slides" and, for Прилог Г, "I write it; you check it" (the option in which the agent writes it).

## Consequences
- Amends 0483: the agent writes the thesis and the slides. AGENTS.md's thesis rule changes to match.
- Differs from the recommendation: the slides too.
- FINKI's template still says AI writing is strictly forbidden, and the committee grades the thesis, not only the mentor. Before writing any of it, the agent asks for the mentor's permission in writing, kept in `D:/thesis` (outside the repo); Прилог Г is the thesis's own statement.
- The kit's notes that say he writes the text (`D:/thesis/README.md`, `outline.md`, `ai-disclosure-facts.md`) follow this record.
