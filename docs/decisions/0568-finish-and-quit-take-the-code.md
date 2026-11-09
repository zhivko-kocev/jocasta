# 0568 Merge, PR and Stop everything take the typed code too

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V11
- Date: 2026-11-09

## Question
Phase B found that any process can start a herdr plugin action, and herdr's socket keys reach an open popup (0558): a thread could start Finish and answer Merge, or start Quit and answer Stop everything. Only Allow takes the 3-letter code. Do Finish and Quit take it too?

## Facts
Found 2026-11-09 in Phase B's build.
- herdr runs a plugin action for `plugin action invoke` from any client, not only his keypress; 0544 took his key as his confirmation.
- Finish's popup offers Merge / PR / Cancel; Quit's close question offers Stop everything / Cancel (0563).
- 0549: a stopped Jocasta resumes mid-turn threads on the next start.

## Options
| Option | Pros | Cons |
|---|---|---|
| Code for Merge, PR and Stop everything | A thread can open these popups but never complete them | A few keystrokes for each |
| Code for Merge and PR only | Merging is the risky one | A thread could stop Jocasta |
| Keep as built | One key each | A thread could finish or quit through herdr |

## Recommendation
Code for Merge, PR and Stop everything.

## Choice
Finish's Merge and PR, and the close question's Stop everything, each take a random 3-letter code typed in the popup, as Allow does (0558); Cancel and Esc stay one key, and closing the popup any other way cancels. Stop (one thread) and New thread stay one key.

## Reason
"Code for Merge, PR and Stop everything".

## Consequences
- Amends 0544 (his key alone no longer confirms Finish), 0558 and 0563.
- The same check as 0558 applies: if a popup's screen turns out readable through herdr, these fall back with Allow.
