# 0224 Approval overlay

- Status: decided
- Kind: product
- Phase/slice: 2/2.4
- Date: 2026-10-08

## Question
Where are approvals answered, and which choices does the overlay offer?

## Facts
- Before phase 4 there is no Frontier; the harness runs in a tab. Plan 4.6 (not yet grilled) says the boss reports a waiting approval and he answers it in the tab.
- `yolo` is the default (0200), so only explicit ask rules and the git-guard asks prompt. A boss that approves on its own would be a classifier-style mode, which 0199 left out.
- With deny, then ask, then allow (0197), an "always" grant (0222) can't override an ask rule; it can only answer an ask from an unmatched call in normal mode (0198).
- His first answer was free text: "jarvis will ask me because he will be the frontier right? or if it makes sense we dont ask then". A follow-up explained the above.

## Options
| Option | Pros | Cons |
|---|---|---|
| Five choices: once, this run, always in this project, deny, deny with a note | — | — |
| Three: once, always, deny with a note | Fewer keys | No "this run" |
| Follow-up: the tab overlay now, the boss relay decided at 4.6 | Nothing decided early | — |
| Follow-up: from phase 4 the boss chat can answer too | — | Decided early |
| Follow-up: the boss approves routine asks | Fewer prompts | An agent approves its crew's risky actions |
| Follow-up: five choices, "always" only where it can apply | Grants stay meaningful | — |
| Follow-up: three choices without "always" | Simpler | 0222 unused |

## Recommendation
Five choices; the tab overlay now.

## Choice
Now (0307): the overlay has no "always" choice (0222 dropped).

Originally: The overlay is in the tab. It offers yes once, yes for this run, yes always in this project (only for asks from unmatched calls), no, and no with a note that goes back to the model. Whether the boss chat can also answer is decided at 4.6.

```
Bash: git push origin feature/login
  ask: Bash(git push *)  (git-guard)
  [1] yes, once   [2] yes, for this run
  [4] no          [5] no, and tell Jarvis…
```

## Reason
"jarvis will ask me because he will be the frontier right? or if it makes sense we dont ask then"

## Consequences
- 2.4's remaining items (headless 0207; reporting to the boss, 4.6) are covered; slice 2.4 is decided.
- Amended by 0307: no "always" choice.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
