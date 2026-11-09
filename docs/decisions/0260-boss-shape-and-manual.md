# 0260 The boss's shape and manual

- Status: decided
- Kind: product
- Phase/slice: 4/4.4
- Date: 2026-10-08

## Question
What is the boss, and what rules does it follow?

## Facts
Read 2026-10-08 in firstmate (kunchenguid/firstmate, commit b062eb9, MIT): the first mate is a stock harness running one contract file, `AGENTS.md` (427 lines). Its hard rules (`AGENTS.md:28-43`): never write to a project; never merge without the captain's explicit word; never tear down unlanded work; crewmates never address the captain; report outcomes faithfully. Standing preferences live in `data/captain.md` (`docs/configuration.md:632-638`).
- With in-process sessions (0250), the boss is one more `AgentSession`; its thread is the home thread (0125). V12: he can open any tab and talk to a thread.
- His answer: "whatever makes most sense for our approach but i would love if we can steal something from there it seams to me like a good project".

## Options
| Option | Pros | Cons |
|---|---|---|
| A Jarvis session with a shipped boss manual carrying firstmate's five rules, adapted; preferences stay in his `AGENTS.md` | Steals firstmate's contract; no second file | — |
| The same plus a boss preferences file the boss curates | Firstmate's `captain.md` | A second preferences file |
| A controller without a model plus a chat model | — | Two systems |

## Recommendation
A firstmate-style boss with five rules.

## Choice
The boss is a Jarvis session in a boss role, with a shipped manual loaded only in its session:
1. Never write to a project; threads change code.
2. Never push or merge without his word.
3. Never discard unlanded work.
4. Threads' questions come through the boss, which relays them (0259); he can still talk to any thread in its tab.
5. Report outcomes plainly, failures with evidence.
It handles open decisions one at a time, highest impact first. His standing preferences stay in `~/.jarvis/agent/AGENTS.md` (0066); there is no boss home folder.

## Reason
"whatever makes most sense for our approach but i would love if we can steal something from there it seams to me like a good project"

## Consequences
- 4.4 items 1, 7 and 8 are decided. The manual's full text is previewed when the build reaches it.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
