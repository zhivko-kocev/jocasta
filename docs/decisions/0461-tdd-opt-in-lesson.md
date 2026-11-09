# 0461 TDD as an opt-in lesson

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-23

## Question
Does Jarvis make threads write tests first?

## Facts
Verified 2026-10-23.
- Red-green stops agents cheating at tests, but only "for places where you can pull it off", with tests at pre-agreed seams (`videos/claude-matt-pocock-24-apr-26.md:253-257`; https://github.com/mattpocock/skills/blob/main/skills/engineering/tdd/SKILL.md).
- omp has no TDD mode; a harness can't verify "red first".
- Ponytail asks for one small test for non-trivial logic (`config/ponytail.md:55`).

## Options
| Option | Pros | Cons |
|---|---|---|
| A shipped TDD lesson a project opts into | Test-first where it fits; the reviewer sees it (0459); no new mechanism | Off unless he adds it |
| An enforced red-first gate in `thread_finish` | Always on | Can't be verified; wrong where TDD doesn't fit |
| Nothing | No change | No way to turn it on per project |

## Recommendation
A shipped TDD lesson a project opts into.

## Choice
Jarvis ships a TDD lesson text (`config/lessons/tdd.md`); a project turns test-first on by adding it to its lessons. Off unless he adds it; Ponytail's one-test rule stays.

## Reason
"A shipped TDD lesson a project opts into".

## Consequences
- Depends on 0459 (lessons).
- The lesson's text is chosen by the builder and listed as built.
