# 0357 Thread tabs are the harness TUI

- Status: decided
- Kind: product
- Phase/slice: 4/4.1, 4.2, 4.8
- Vision: V5, V7
- Date: 2026-10-09

## Question
In real use he expected the harness TUI itself in Frontier's tabs, not a web rendering of the session (0250). How do the tabs work?

## Facts
Checked 2026-10-09.
- 0250 and slice 4.2 decided in-process Pi sessions rendered by Frontier's web page, with `jarvis --thread <id>` for the TUI in a terminal. That is what was built.
- herdr (https://github.com/herdrdev/herdr, README): it "doesn't wrap or replace [agents]; it owns their terminals"; terminals run in a background server and survive a closed client; "after a server or machine restart … the original processes do not survive"; agents drive it "through the cli and socket api"; panes are marked working, blocked or idle from their output.
- Bun 1.4.2 has a built-in PTY: `Bun.spawn(cmd, { terminal })` / `Bun.Terminal`, using ConPTY on Windows (`bun-types` docs, `child-process.mdx`). ConPTY re-encodes output and passes `\r` through unchanged. node-pty isn't needed.
- npm today: `@xterm/xterm` 6.0.0, `@xterm/addon-fit` 0.11.0, `@xterm/addon-webgl` 0.19.0, `@xterm/addon-unicode11` 0.9.0, all MIT.
- Today every boss, HUD, voice, card, canvas and spend feature is fed by in-process session hooks in `src/frontier/server/host.ts` (the review of 2026-10-09 lists them); a harness in its own process emits none of them, so each needs an IPC equivalent.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep 0250, TUI tabs after v1 | No rework | Not what he expects |
| TUI tabs over PTY + IPC now | The real harness in every tab; crash isolation per thread | xterm.js installs; one process per running thread; rework of 4.1/4.2 and every in-process hook |
| herdr itself | Mature terminal host | Reverses 0249; an install |

Follow-ups, each with its own options:
- Who owns the terminals: Frontier's server | a separate background host, herdr-style.
- Quit: asks "Close Frontier" or "Stop threads too" | closes Frontier only, host stops when idle | server-owned after all.
- Tab content: TUI only, questions in the TUI or the boss chat | TUI only, questions only in it | TUI plus a slim strip.
- Dictation in a TUI tab: hold Ctrl+Space | mic button only | hold Space when the TUI input is empty.
- Install: all four xterm packages | xterm + fit only.

## Recommendation
Keep 0250 for v1 and grill TUI tabs after v1. Then: Frontier's server owns the terminals; TUI with questions in the TUI or the boss chat; Ctrl+Space dictation; all four packages.

## Choice
- Thread tabs are the real harness TUI: each thread runs `jarvis --thread <id>` in a PTY (Bun's ConPTY terminal), shown with xterm.js in Frontier's page. A Jarvis extension inside each harness talks to Frontier over local IPC: it reports states, events, questions, approvals, drawings, spend and reports, and takes steering, answers and stop. He asked "is ipc the right approach here? herdr works like that?" and, after the facts, chose this now.
- herdr-style: a background Jarvis host owns every thread's terminal; Frontier's server and window attach to it. Threads keep running through a Frontier crash, a Quit or an update; a PC restart still ends them, and they come back as interrupted (0256). His words: "isnt it better with herdr style".
- Quit asks "Close Frontier" (the default; threads keep running in the host) or "Stop threads too" (ends the host; running threads are marked interrupted).
- A tab shows the TUI with a one-line status strip above it (state, model, cost). Questions and approvals can be answered in the TUI or from the boss chat's card; the first answer wins and the other closes (0269 kept). His words: "the recommended + the strip".
- Dictation stays in the boss chat (hold Space, 0273); TUI tabs have none. His words: "i meand when holding space in the boss, usually we will go into tui's if they are stuck or the situation is fire".
- Installs: `@xterm/xterm` 6.0.0, `@xterm/addon-fit` 0.11.0, `@xterm/addon-webgl` 0.19.0, `@xterm/addon-unicode11` 0.9.0.

## Reason
His words, quoted above.

## Consequences
- Amends 0250 and slice 4.2 (tabs), 0253 (Quit), 0273 (dictation in every tab) and 4.1's process model.
- Everything `host.ts` hooks in process moves behind IPC: thread states and settling, done/failed toasts and speech, the running limit (0268), spend and the HUD (4.12), `report` and `remember`, every prompt card with its blocked state, speech, urgent event, reminder and decision log (0269, 0280, 0303), the trust card (0333), live drawings (0295), and the boss's tools (`thread_start`, `thread_send`, `thread_stop`, `thread_status`, the digest).
- One process owns a thread at a time; the session file has a single writer.
- The container manager has one owner, so one harness exiting doesn't stop Graphiti for the others.
- A workspace thread gets its brief as its first message however it was created (0123).
- Names (host process, IPC channel, env vars, the Quit dialog's text, the strip's layout) are built minimally and confirmed with him as built.
- Differs from the recommendation, which kept 0250 for v1 and had Frontier's server own the terminals.
- Amended by 0370: a thread's TUI shows on the right while the boss chat stays visible on the left.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
