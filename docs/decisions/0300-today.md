# 0300 The today view

- Status: decided
- Kind: product
- Phase/slice: 8/8.4
- Date: 2026-10-08

## Question
What does `today` show, and when?

## Facts
- The boss digest has four sections (0263); the nightly consolidation proposes memory changes (0281); calendar, GitHub and Azure DevOps events arrive through 8.1.

## Options
| Option | Pros | Cons |
|---|---|---|
| A Frontier view with Calendar, Needs you, Threads and Overnight; opens on the first launch each morning | One morning glance | — |
| The same without auto-open | Less intrusive | Easy to skip |
| A CLI `today` command too | Appendix I claims `today` | More surface |

## Recommendation
As described.

## Choice
A view in Frontier, from a HUD button or by asking the boss "what's today?": Calendar (today's meetings), Needs you (reviews requested, red pipelines, blocked threads), Threads (running, and finished since yesterday) and Overnight (memory proposals from the nightly run). It opens on the first launch each morning. No CLI command.

```
TODAY · Thu 8 Oct
Calendar   10:00 standup (client-a) · 14:00 1:1
Needs you  PR #42 review (acme/api) · pipeline red: client-a/web main
           api/export blocked: "v1 or v2?"
Threads    2 running · 3 finished since yesterday
Overnight  4 memory proposals to confirm
```

## Reason
Not given.

## Consequences
- Appendix I drops the `today` command.
