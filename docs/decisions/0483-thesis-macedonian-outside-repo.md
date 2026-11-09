# 0483 The thesis: Macedonian, outside the repo, written by him

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-24

## Question
In which language and where is the thesis written, and what may agents do for it?

## Facts
Verified 2026-10-24.
- The topic goes through diplomski.finki.ukim.mk, filed by the mentor ("Нова пријава": Тема, Област, Кратка содржина).
- The official Word template is A4; margins 2.5 cm, 3.5 cm at the bottom; Times New Roman 12 pt; 1.5 spacing; IEEE citations.
- The template requires an abstract, contents, introduction, conclusion and bibliography; its order is Апстракт (about 200 words) + Клучни зборови, Вовед, Сродни истражувања, Методологија, Резултати, Заклучок, Библиографија, Прилози.
- The template says, verbatim: "Користењето на вештачка интелигенција за пишување или значително помагање при пишувањето на дипломската работа е строго забрането."
- UKIM's AI declaration (28.11.2024) allows responsible, cited AI use that the professor permits, and advises students to ask their professors what is allowed.
- No explicit language rule for the bachelor's thesis was found; the official template is in Macedonian.

## Options
Language:

| Option | Pros | Cons |
|---|---|---|
| Macedonian | The template's language; no permission needed | Sources and terms are mostly English |
| English | Matches the sources and the repo | Needs his mentor's agreement |

Location:

| Option | Pros | Cons |
|---|---|---|
| `D:/thesis`, outside the repo | His name, index number and mentor never reach the public repo (principle 6) | A second folder to back up |
| In the repo | One place with the code | The public repo would carry personal data |

## Recommendation
Macedonian; `D:/thesis`, outside the repo.

## Choice
- The thesis is in Macedonian, in FINKI's official Word template, kept in `D:/thesis/` outside the repo (it carries his name, index number and mentor; principle 6).
- **He writes every sentence** of the thesis and the slides. Agents prepare only:
  - the template formatted with FINKI's required headings;
  - a proposed outline for his mentor;
  - figures (architecture diagrams, screenshots), benchmark data, tables and charts;
  - the bibliography in IEEE form from sources actually read;
  - the sourced landscape and questions for his outlook chapter (`local://outlook-sources.md`);
  - the list of facts for his AI-use disclosure appendix;
  - for the defense, a slide template, diagrams, charts and a recorded demo.
- The email to his mentor: agents give title options and a draft description; he rewrites it in his own words and asks his mentor how AI tools may be used and disclosed.
- AGENTS.md gains the rule: agents never write thesis or slide text.

## Reason
"Macedonian", "D:/thesis, outside the repo", and FINKI's rule.

## Consequences
- AGENTS.md's conventions gain the thesis rule.
- 0482's raw results live in `D:/thesis/data/`.
- Amended by 0507: his mentor allows AI writing, so once everything else is finished the agent writes the thesis and the slides, and Прилог Г says so (written by the agent, checked by him).
