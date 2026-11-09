# 0407 Notifications from one place

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-13

## Question
Who raises notifications?

## Facts
Verified 2026-10-13.
- Proposal: `docs/research/ux-proposal.md` (UX pass, 0395).
- herdr's automatic toasts and its `notification.show` share one switch, `[ui.toast] delivery = off | herdr | system` (default off; `herdr:src/config/model.rs:1252`); sounds are `[ui.sound]`.

## Options
| Option | Pros | Cons |
|---|---|---|
| One place: the service and the boss | One conductor; quiet hours and urgency apply to everything | herdr's own toasts off |
| Voice and boss only | Quietest | No visual alert away from the terminal |
| herdr for thread states, service for the rest | herdr's click-to-pane | Two sources |

## Recommendation
The service and the boss.

## Choice
herdr's toasts and sounds are off. Every event goes through the service's queue: the boss's `[event]` line, the spoken line (0405), and a companion Windows toast for urgent and blocked ones; clicking it focuses that pane in herdr.

## Reason
"the idea is the boss to be the dirigent maybe it is better to handle from one place"; then "One place: the service/boss".

## Consequences
- Amends 0329 and 0271: one source of notifications.
- Amended by 0519: the urgent and blocked toasts, while the window is hidden or behind, are Jocasta's own windows at the display's bottom-right corner instead of Electrobun's balloon; clicking one focuses that pane in herdr.
- Amended by 0552: no app; Windows notifications ping, questions are answered in the `jocasta` pane.
- Amended by 0554: herdr delivers the service's notifications to Windows (`ui.toast.delivery = "system"`), with a sound when a thread needs him.
