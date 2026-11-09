# 0527 The thesis gives the playbook and omp² their own sections and cites three voices of the new wave

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
While 0520's reruns ran he asked whether the thesis can also cover the analysis of omp²'s approach and Stencil's Harness Playbook ("i think all things are very useful"). 0511 makes the playbook's five consequences the checklist chapters 3 and 6 score Jocasta against. Where do the playbook and omp² go, and do the other 2026-11-01 sources missing from the evidence pack go in?

## Facts
Verified 2026-11-02.
- The outline for his mentor (`D:/thesis/outline.md`) names neither the playbook nor omp². `D:/thesis/sources/playbook-explained.md` explains the playbook and scores Jocasta against each rule, each with "After the move to omp²".
- omp² is unreleased: npm's `@oh-my-pi/pi-coding-agent` has one dist-tag, `latest: 18.15.0` (https://registry.npmjs.org/-/package/@oh-my-pi/pi-coding-agent/dist-tags). The thesis can analyse its design, not measure it.
- FINKI states no page count; the outline's 50–60 pages is an inference (`D:/thesis/sources/finki-thesis.md` §5).
- The evidence pack carries HumanLayer, Loka and Destiner (chapter 2), but not three opinion pieces on the new way of working: Daniel Miessler's "Writing Is Closer to Thought Than Code", Sebastian Schürmanns's "Tasks as Code" (Typemill) and Niko Matsakis's "What agents mean for Rust (and vice versa)" (bibliography [236]–[238]; `D:/thesis/sources/proposals-2026-11/thought-tasks-rust.md`).

## Options
| The playbook and omp² | Pros | Cons |
|---|---|---|
| Sections in the chapters the outline has | The playbook as related work, the checklist where 0511 puts it, omp² as Jocasta's next step; the thesis stays on Jocasta and its measurements; about 3 pages more | omp² gets one section, not a chapter |
| A chapter of its own | The most room for the playbook, omp² and his philosophy | About 8 pages more; a long analysis of someone else's unreleased product can read as off-topic, and none of it can be measured |
| 0511 as it stands | No new sections | The playbook is never explained on its own; omp² becomes one paragraph of future work |
| An appendix | Everything in full; the main text stays on the measurements | Read least; the argument sits outside the main line |

| The three voices | Pros | Cons |
|---|---|---|
| In the section on the future of the profession | Named voices of the same wave beside his view, which that section marks as opinion; ties to 0513 and 0514 | About a page more; blog posts, cited as opinion |
| Only where they touch Jocasta | Shorter | Miessler's point, the closest to the title, is left out |
| Left out | The section keeps its sources | His view stands with fewer voices beside it |

## Recommendation
Sections in the chapters the outline has; the three voices in the section on the future of the profession.

## Choice
- **The playbook and omp²:** chapter 2 gains 2.7 "Харнесот како системски софтвер: Harness Playbook и omp²": the four ways a harness is used, the five rules, omp's postmortem, and omp² as its design, with its release state. 3.2 carries the five rules as the checklist (0511). Chapter 6 becomes 6.1 Научени лекции; 6.2 Jocasta низ петте правила, where it follows each rule and where it doesn't; 6.3 Патот кон omp², what the move changes (0514's break list, 0515's principle), design-level and not measured; 6.4 Ограничувања; 6.5 Иднината на AI алатките и на професијата. Прилог Д holds the rule-by-rule table with its `path:line` evidence. Page targets: chapter 2 9–11, chapter 6 6–8.
- **Three voices of the new wave** go into 6.5 beside his view, cited as opinion: Miessler (the work moves to writing down intent), Schürmanns (tasks as files in the repository, beside 0513's task file per thread) and Matsakis (types and checks as agents' guardrails, beside 0514's Rust question).

## Reason
"Sections in the chapters we have"; "Into the future-of-the-profession section".

## Consequences
- Amends 0511: the playbook is explained in 2.7, and the checklist gets its own section (6.2) and appendix (Прилог Д).
- The outline and the evidence pack follow: chapter 6's 6.2 and 6.3 become 6.4 and 6.5.
- If omp² is released before the thesis is written, 6.3's facts are checked again then; the move itself still gets its own record (0514).
- Nothing changes in Jocasta or in the runs.
