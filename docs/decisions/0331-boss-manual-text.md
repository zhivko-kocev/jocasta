# 0331 The boss manual's text

- Status: decided
- Kind: product
- Phase/slice: 4/4.4
- Date: 2026-10-08

## Question
What does the boss's shipped manual say? (0260: previewed at build)

## Facts
- Built from 0260's five rules and 0261's tools. He asked to study firstmate (https://github.com/kunchenguid/firstmate @19fcbbd, MIT): its `AGENTS.md` has the same five hard rules plus intake (resolve the project per request, one confident match or one question), ship as default and scout only when uncertainty could change what to build, briefs in the captain's own words, outcomes not mechanics, an immediate-escalation list, and captain instruction precedence that is never broadened.
- He asked why the revision is better; each added line was tied to a failure it prevents.

## Options
| Option | Pros | Cons |
|---|---|---|
| Revised manual with firstmate's intake, escalation and wording rules | Fewer wrong projects, scouts, widened briefs and noisy reports | ~12 more prompt lines |
| Original five rules and tools | Shortest | Says little about doing the job |

## Recommendation
Revised.

## Choice
The manual in `src/frontier/boss/manual.ts`: the five rules unchanged, then intake, ship as default, briefs from his own words, parallel starts, what to tell him at once and what to batch, outcomes in his words, one decision at a time, and his explicit instruction overriding a rule for exactly what he named.

## Reason
"https://github.com/kunchenguid/firstmate - can you analyze this and maybe we can use something from here also"; "why do you suggest the new .md and why is it better than the original?"

## Consequences
- firstmate is a reference only; nothing was copied verbatim beyond its five rules, which 0260 already had.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0465: procedures the boss rarely needs move into skills under `<data>/home/.omp/skills/`; the manual keeps what must always hold.
