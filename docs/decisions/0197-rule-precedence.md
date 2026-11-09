# 0197 Rule precedence

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
How do rules from the user file and a project file combine?

## Facts
Verified 2026-10-08 (https://code.claude.com/docs/en/permissions): Claude Code evaluates deny, then ask, then allow; the first match in that order wins; a user-level deny blocks a project-level allow; an allow can't carve an exception out of a deny.
- opencode: the last matching rule wins (lesson L48), so a later project file can weaken a user deny (lesson L25).
- Principle 4: fail closed on what guards.

## Options
| Option | Pros | Cons |
|---|---|---|
| Deny, then ask, then allow, across every layer | No project file can weaken his denies; the same model as his Claude Code setup | A narrow exception needs a narrower deny |
| Last match wins | Easy exceptions | A cloned repo's file can switch denies off |

## Recommendation
Deny, ask, allow across layers.

## Choice
Deny, then ask, then allow, across the user file, the project file and profiles; the first match in that order decides. An allow never overrides a deny or an ask from any layer.

## Reason
Not given.

## Consequences
- Appendix A's rule-precedence row (0077) is decided.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
