# 0250 What runs threads behind Frontier

- Status: decided
- Kind: product
- Phase/slice: 4/4.1, 4.2
- Date: 2026-10-08

## Question
What runs each thread behind Frontier's UI?

## Facts
- Pi's SDK exposes `createAgentSession`, `AgentSession` and `AuthStorage` (appendix B), so one process can host many thread sessions and draw them itself.
- Verified 2026-10-08 (https://herdr.dev/docs/windows-beta/): herdr runs a real terminal per pane; direct terminal attach into another UI is unsupported on Windows.

## Options
| Option | Pros | Cons |
|---|---|---|
| In-process Pi sessions, rendered by Frontier | One process; no terminal emulation; native rendering of messages, tool calls, diagrams and approvals | Frontier draws everything |
| herdr as runtime | Persistent panes and states for free | Two UIs; an install |
| Pseudo-terminals in the UI | Pi's own TUI per tab | Terminal emulation |

## Recommendation
In-process Pi sessions.

## Choice
Now (0357): thread tabs are the real harness TUI. Each thread runs `jarvis --thread <id>` in a PTY owned by a background Jarvis host (herdr-style), shown with xterm.js. Frontier talks to each harness over IPC.

Originally: In-process Pi sessions: Frontier hosts each thread's `AgentSession` and renders it. `jarvis --thread <id>` still opens any thread in Pi's terminal UI for hand-work. herdr is a design reference (agent states, persistence, notifications).

## Reason
"the recomended, we can use herdr for reference maybe"

## Consequences
- 4.2's tab-layer options are decided; 4.1's process model is grilled next with this as a fact.
- herdr joins the references beside firstmate (0217) and whiteboard (0249).
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
