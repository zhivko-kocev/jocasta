# 0198 Calls no rule matches

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
What happens to a call no rule matches?

## Facts
- Claude Code's `default` mode prompts on first use of each tool. He ran Claude Code with `Bash(*)` allowed and omp in `yolo`.
- Unattended runs can't answer an ask (exit 3, 0140).

## Options
| Option | Pros | Cons |
|---|---|---|
| Reads and writes inside the project allowed; exec, net, MCP and anything outside the project ask | Safe for a new install | Prompts until allow rules are added |
| Allow everything not denied or asked | No prompts | An unforeseen dangerous command runs |
| Ask for everything | Safest | Unusable without many rules |

## Recommendation
Allow in-project, ask otherwise.

## Choice
Reads and writes inside the project (its paths, 0104) are allowed; exec, net, MCP and anything outside the project ask unless a rule allows them.

## Reason
Not given.

## Consequences
- A headless run hitting an ask exits 3 (0140) until resumable runs (0141).
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
