# 0257 Frontier layout

- Status: decided
- Kind: product
- Phase/slice: 4/4.3
- Date: 2026-10-08

## Question
How is Frontier's web UI laid out?

## Facts
- The web UI (0251) keeps the sidebar, tabs and boss of the original picture and adds the HUD (4.12) and a diagram canvas (V6, 0249). The sidebar lists the projects from the watched roots (V4, 0102).

## Options
| Option | Pros | Cons |
|---|---|---|
| Three columns: projects; boss or thread tab; a canvas that opens when something is drawn | Everything at a glance | Width |
| Two columns, the canvas as a tab | Simpler | Less at a glance |

## Recommendation
Three columns.

## Choice
A HUD strip on top; three columns below: projects with their threads and states on the left, the boss chat or a thread tab in the middle with the input line and the mic hint, and a canvas on the right that slides in when an agent draws and hides otherwise. Only active threads get tabs.

```
┌─ HUD ── ◉ 2 running · ⚠ 1 blocked · $3.10 today · 62% usage left · ● mic ──────────┐
│ PROJECTS        │ [boss] [demo · add retry ●] [api · export !]  │ CANVAS          │
│ ▾ demo         │ boss › demo/add retry is running tests.        │  ┌───┐   ┌───┐  │
│   project       │        api/export is blocked: "v1 or v2?"     │  │cli│──▶│api│  │
│   ● add retry   │ you  › tell api to use v2                      │  └───┘   └───┘  │
│ ▾ api          │ boss › sent. demo finished: 2 commits.         │  sequence: login │
│   ! export      │                                                │  (click → code)  │
│ ▸ web          │ ── hold space to talk · type to chat ──        │                  │
└─────────────────┴────────────────────────────────────────────────┴──────────────────┘
```

## Reason
Not given.

## Consequences
- Key bindings beyond the previewed hold-space are 4.8's and the build's previews.
- Amended by 0370: Jarvis on the left with the crew of thread pills; the canvas or a thread's TUI on the right; the projects sheet replaces the sidebar.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0395: the look is re-decided in the UX pass before F2.
