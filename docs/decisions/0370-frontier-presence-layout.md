# 0370 Frontier's layout: Jarvis presents his crew

- Status: decided
- Kind: product
- Phase/slice: 4/4.3
- Vision: V1, V4, V5, V6
- Date: 2026-10-09

## Question
With the JARVIS look (0369), how is Frontier laid out? 0257 has a projects sidebar, the boss chat or a thread tab in the middle, and a canvas column that slides in.

## Facts
- 0257: HUD strip; projects with their threads on the left; the boss chat or a thread tab in the middle; the canvas on the right when an agent draws. 0357: a thread tab is the harness TUI under a one-line status strip.
- V4 shows the projects "in a sidebar or similar". V5's tabs "may work like herdr's".
- herdr (read 2026-10-09): tabs carry their agent's state; blocked comes first; a state change draws once and nothing animates after.
- On impeccable's decision page he saw three structures dealt at random from a ranked list: everything behind a command palette, a workbench of tiled terminals, and the Presence (the conversation first, a thread dock, the canvas beside it). He re-rolled with a steer, then locked one of the second hand.

## Options
| Option | Pros | Cons |
|---|---|---|
| Tabs across the top: a full-width tab bar with Jarvis as the home tab | Familiar browser tabs | Two rows of chrome; Jarvis narrows on a thread |
| Tabs at Jarvis's feet, under the input | The crew sits where he types | Less familiar |
| Tabs under the presence: pills under the greeting; the right side shows the canvas or the chosen thread | Jarvis introduces the crew; the conversation never leaves | Tabs on the left steer the right side |

## Recommendation
None: the decision page deals its structures by roll, without a pick.

## Choice
Tabs under the presence.
- Under the HUD, two work areas. Left, always: Jarvis. The greeting (the Presence, "Good morning." by the hour, and one sentence on who is at work and who would like a word), the crew, the boss chat, his requests docked above the input, the input and its hints.
- The crew is a row of pills: Canvas first; then one pill per open thread tab and per thread at work, starting, waiting on him, queued, failed or interrupted, each with its state mark; a Projects pill last. Ctrl+1 to Ctrl+9 picks a pill.
- The Projects pill opens a sheet over Jarvis's column with every watched project, its git prep and inbox notes, and its threads; choosing a project opens its project thread (0276), choosing a thread opens its tab.
- Right: the canvas, or the chosen thread's TUI under its status strip (0357). A drawing that arrives while a thread is shown marks the Canvas pill instead of taking the right side. Closing the canvas gives Jarvis the whole width; the right side opens again when a thread or the canvas is chosen.
- Today (0300) and the projects sheet cover Jarvis's column while open.
- A thread's TUI takes about 60% of the width for its columns; the canvas about 44%.
- In-app toasts (0329) are rows in Jarvis's dock above his requests, never a layer over the canvas.

```
┌ ◎ JARVIS  3 at work  1 awaiting you  $4.82 today  62% left ─────────── 14:32  ▢ ▤ ≡ ◖ ⏻ ┐
│  ◎  Good afternoon.                              │ Canvas · acme-api              Tour  × │
│     Three threads at work. One would like a word.│ POST /orders under the new limiter     │
│ (▢ Canvas)(● Rate limiter)(● Flaky checkout)(…)  │ ┌──────┐   ┌───────┐   ┌───────────┐   │
│  ┌ How is acme-api doing? ┐                      │ │Client│──▶│Gateway│──▶│RateLimiter│   │
│  ◎ Two threads. Rate limiter is running…         │ └──────┘   └───────┘   └───────────┘   │
│ ┌ ● acme-api  Fix flaky checkout test ─────────┐ │ 41  export async function take(…)     │
│ │ Freeze time, or fix orderDate()?  [1] [2] [3]│ │                                        │
│ └──────────────────────────────────────────────┘ │                                        │
│ ◎ [ Speak or type to Jarvis                    ] │                                        │
└──────────────────────────────────────────────────┴────────────────────────────────────────┘
```

## Reason
His steer on the decision page: "i would love the presence with also the threads visible only the tabs but i love the idea right side canvas and stuff and left jarvis agent with maybe the threads". He then locked "Tabs under the presence".

## Consequences
- Amends 0257: no projects sidebar (the Projects sheet keeps V4); the canvas shares the right side with thread tabs.
- Amends 0357's placement: a thread's TUI shows on the right while the boss chat stays visible on the left. Holding Space still dictates only in the boss chat, and Right Ctrl anywhere (0273, 0368).
- The Projects sheet, which threads get a pill, the drawing mark on the Canvas pill and Ctrl+1–9 were chosen by the builders under 0363, to keep V4 and the tab hint of the sketch he locked.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0395: the look is re-decided in the UX pass before F2.
- Amended by 0408: Alt+1–9 switch herdr workspaces instead of Ctrl+1–9.
- Amended by 0409: the companion is a rail beside the canvas; the crew pills and boss chat leave the window.
- Amended by 0555: the Presence orb and the greeting go.
