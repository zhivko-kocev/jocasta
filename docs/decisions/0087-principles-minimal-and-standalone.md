# 0087 Principles: minimal by default, the harness stands alone

- Status: decided
- Kind: process
- Phase/slice: 0/section 4
- Vision: V7, V8, V12
- Date: 2026-10-07

## Question
Which of two principles from his direction join section 4's principles (0025)?

## Facts
- His words on 2026-10-07: "i want minimalistic but still with the best features out of the harnesses custom made tailored for me" and "jarvis that orchestrates minimalistic harness instacess and the harness can be used also alone".
- Principle 1 ("fewer knobs") covers settings, not features.
- 0034: bare `jarvis` opens the harness until phase 4, then Frontier.
- Pi's design is a small core with policy in extensions.

## Options
| Option | Pros | Cons |
|---|---|---|
| Both | The grilling program has a rule to cut by; Frontier never becomes a dependency of the harness | Phase 4 must keep a way to open the harness alone (1.6, 4.10) |
| Minimal only | Cuts features | Standalone use isn't guaranteed |
| Standalone only | The harness works alone | No cutting rule |
| Neither | — | — |

## Recommendation
Both.

## Choice
Both:
8. **Minimal by default.** The harness is Pi plus the features he picked; a feature he didn't pick is left out, not shipped switched off.
9. **The harness stands alone.** Every harness feature works without Frontier, and the harness can be opened on its own after phase 4 (how is decided in 1.6 and 4.10). Frontier orchestrates harness instances and adds only orchestration: projects, threads, tabs, the boss, voice and pollers.

## Reason
His words, quoted in Facts.

## Consequences
- Plan section 4 lists principles 8 and 9.
- 1.6 and 4.10 decide how the harness opens alone once bare `jarvis` opens Frontier (0034).
