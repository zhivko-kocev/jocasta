# 0217 firstmate as a phase 4 design reference

- Status: decided
- Kind: product
- Phase/slice: 4/4.4–4.6 (raised during phase 2)
- Date: 2026-10-08

## Question
Zhivko found firstmate and asked whether it can simplify Jarvis. How does it enter the plan?

## Facts
Inspected 2026-10-08: repo kunchenguid/firstmate at commit b062eb9, MIT.
- He talks to one agent, the first mate. It spawns workers, each in its own git worktree and its own tmux window or herdr tab, supervises them, and returns PRs, local merges or investigation reports (README "How It Works").
- It is an "agent distro": a repo holding `AGENTS.md` (427 lines), skills and scripts, opened in a stock harness. Pi is a supported primary, with tracked `.pi/extensions/*.ts` loaded after Pi's trust prompt (README).
- A bash watcher sleeps on the fleet and wakes the first mate only for actionable events, so waiting costs no tokens (`docs/architecture.md`). Its Pi guard blocks bash commands through `tool_call` (`.pi/extensions/fm-primary-turnend-guard.ts:590-600`).
- Built-in skills: `/ahoy` (recap since his last message and open decisions), `/bearings` (fleet digest), `/afk` (away mode), `/quiet`, `/stow` (README "Built-in skills").
- Voice runs through AWS Bedrock Nova Sonic over SSH (`docs/voice-relay.md:22,90-100`).
- Platforms: macOS and Linux (README badge); some scripts handle Git Bash/MSYS (`bin/fm-watch-arm.sh:133-136`). Backends: tmux is the verified default, herdr has a CI lane, zellij, Orca and cmux are experimental (`docs/configuration.md:417-423`); worktrees come from Treehouse (`docs/configuration.md:425`). It needs `gh`.
- Size, measured: 463 `.sh` files with about 339,000 lines of shell, 29 `.ts` files.

## Options
| Option | Pros | Cons |
|---|---|---|
| A design reference for phase 4, with catalog rows | Borrows proven ideas; keeps Frontier (V1, V3, V4) | Jarvis still builds phase 4 |
| Run it in WSL as Jarvis's boss | A working boss soon | Not native Windows; tmux or herdr instead of Frontier's TUI; bash he doesn't own |
| Not used | — | — |

## Recommendation
A design reference for phase 4.

## Choice
A design reference for phase 4. Appendix A gains its rows, and slices 4.4–4.6 cite it among their options.

## Reason
Not given.

## Consequences
- New appendix A rows: the zero-token watcher; recap on return and fleet digest; away mode; the turn-end guard; ship and scout task shapes.
- Its code is not copied (principle: references are read-only).
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
