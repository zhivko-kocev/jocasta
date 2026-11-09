# 0188 Slice 2.1 agenda

- Status: decided
- Kind: process
- Phase/slice: 2/2.1
- Date: 2026-10-07

## Question
Which decisions does slice 2.1 (permission engine) grill, now that v4 2.1's decisions 1–18 are decided fresh (0088)?

## Facts
Verified 2026-10-07 in Pi 1.0.1: a throwing `tool_call` handler blocks the tool (`dist/core/agent-session.js:319-333`); a throwing `user_bash` handler blocks the `!` command (`dist/core/extensions/runner.js:973-999`); Pi keeps per-folder trust in `<config dir>/trust.json` (`dist/core/trust-manager.js:171-177`).
- Lessons mapped to phase 2: L04 (2.6), L24 (phase 2), L25 (2.1, 2.4), L26 (2.1, 2.2), L27 (2.1), L28 (2.5), L36 (2.7), L43 (2.1), L48 (2.1), L68 (2.1's parser dossier).
- Appendix A rows naming 2.1: allow/ask/deny rules, permission modes, rule precedence (0077), capability tiers (0077), OS sandbox, history-aware rules (0082), `!` shell commands.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of thirteen: policy file; rule grammar; layers; precedence; default for unmatched calls; permission modes; capability tiers and critical patterns; shell parser as a spike rule; `!` commands; sandbox; history-aware rules; a broken policy file; `ask` rules headless | Every lasting guard choice and catalog row is asked | Thirteen questions |
| Change it | — | — |

## Recommendation
The fresh agenda.

## Choice
The fresh agenda of thirteen.

## Reason
Not given.

## Consequences
- Plan 2.1's list becomes the thirteen items, citing this record.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
