# 0085 The learning repo as a source

- Status: decided
- Kind: process
- Phase/slice: 0/section 4
- Date: 2026-10-07

## Question
Do phase openings and grilling sessions use `D:/learn-harnesses`?

## Facts
Verified 2026-10-07.
- `D:/learn-harnesses` holds pinned clones of Pi v1.0.1, omp v18.6.0, opencode v1.18.22, Claude Code v2.1.280, Codex rust-v0.160.1, aider v0.86.0, mini-swe-agent v2.4.6 and the Claude Agent SDKs (`repos.lock.json`). Records 0077 and 0078 were verified against them.
- `JARVIS-MAP.md` maps all 70 lessons to plan slices and the question each prepares; L68 builds dossiers for 1.8, 2.1 and 3.4; L15 benchmarks edit formats, L64 tests Windows process trees, and L69 prototypes 1.8 on Pi 1.0.1.
- Plan section 4's phase-opening sources were npm metadata, Pi's docs and source at the pinned tag, and the official docs of external tools. The reference-files rule named only `D:/friday-harness/` and the v4 and v5 plans.

## Options
| Option | Pros | Cons |
|---|---|---|
| Clones and lessons | One pinned code base for every comparison; each slice's grilling starts from its mapped lessons | Lessons are teaching material, so a claim is re-read in the pinned code before it is cited; the pins can trail Jarvis's Pi pin |
| Clones only | Pinned code without lesson claims | The dossiers and benchmarks go unused |
| Nothing | — | Each fact check re-finds its sources |

## Recommendation
Clones and lessons, read-only like the other references.

## Choice
Clones and lessons, read-only.

## Reason
"i want to decide everything upfrontn now we have good resources sources links repos"

## Consequences
- Plan section 4: the phase-opening sources add the pinned clones; a grilling question's evidence adds the lessons `JARVIS-MAP.md` maps to the slice, re-read in the pinned code before they are cited; `D:/learn-harnesses/` joins the read-only reference files.
- AGENTS.md's reference-files rule names `D:/learn-harnesses/`.
- The answer was free text. It was read as "clones and lessons", and that reading was stated in the next question (0086) without objection.
