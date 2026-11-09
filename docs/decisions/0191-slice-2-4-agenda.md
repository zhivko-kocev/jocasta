# 0191 Slice 2.4 agenda

- Status: decided
- Kind: process
- Phase/slice: 2/2.4
- Date: 2026-10-07

## Question
Which decisions does slice 2.4 (approvals and grants) grill, now that v4 2.4 is decided fresh (0088)?

## Facts
Verified 2026-10-07 in Pi 1.0.1: a throwing `tool_call` handler blocks the tool (`dist/core/agent-session.js:319-333`); a throwing `user_bash` handler blocks the `!` command (`dist/core/extensions/runner.js:973-999`); Pi keeps per-folder trust in `<config dir>/trust.json` (`dist/core/trust-manager.js:171-177`).
- Lessons mapped to phase 2: L04 (2.6), L24 (phase 2), L25 (2.1, 2.4), L26 (2.1, 2.2), L27 (2.1), L28 (2.5), L36 (2.7), L43 (2.1), L48 (2.1), L68 (2.1's parser dossier).
- The dry-run preview row (0081) names 2.4.

## Options
| Option | Pros | Cons |
|---|---|---|
| Fresh agenda of six: overlay choices; where grants live and how "always" matches; expiry and revocation; the dry-run preview row; headless; reporting to the boss | Grants' matching rule is decided explicitly | — |
| Change it | — | — |

## Recommendation
The fresh agenda.

## Choice
The fresh agenda of six.

## Reason
Not given.

## Consequences
- Plan 2.4's text becomes the agenda, citing this record.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
