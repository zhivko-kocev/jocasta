# 0329 In-app toasts and browser notifications

- Status: decided
- Kind: product
- Phase/slice: 4/4.9
- Date: 2026-10-08

## Question
How do toasts look and work?

## Facts
- Browser notifications are drawn by the OS as standard toasts and can't be styled. 0271: a toast with a sound when a thread blocks or finishes and Frontier isn't focused; clicking opens the thread.
- He asked whether browser notifications are stylish and wants multi-OS support.

## Options
| Option | Pros | Cons |
|---|---|---|
| In-app styled toasts, plus browser notifications when unfocused | Styled in the app; works on every OS | OS notifications are plain |
| Windows toasts only | — | Windows only; unstyled |

## Recommendation
In-app plus Windows toast.

## Choice
Frontier shows its own styled toasts when its window is focused; when it isn't, the page raises a browser notification (any OS), and clicking it opens the thread.

## Reason
"in app stylet toasts + maybe browser notifications when unfocused because we want multiple os support maybe?"

## Consequences
- No install; he allows notifications once in the window.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0407: herdr's toasts are off; the service raises every notification.
- Amended by 0552: no app; when the terminal isn't in front a Windows notification pings him, and the answer is given in herdr.
