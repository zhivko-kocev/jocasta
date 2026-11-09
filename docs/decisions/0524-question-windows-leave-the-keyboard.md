# 0524 The Allow window and the close question leave the keyboard with his app

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-11-02

## Question
0519's Allow window and close question take the keyboard when they open. R1's system-modal box left it with the app he works in. Which should they do?

## Facts
Verified 2026-11-02 in the VM's benchmark sandbox, on the build with 0518–0523 (`55763c6b…02d3`).
- With Notepad in front and receiving keys, the Allow window opened on top, above Notepad, and became the foreground, so Notepad lost the keyboard. `askInWindow` (`src/app/main.ts`) shows the window and then calls `activate()`.
- R1's system-modal box (wave 2's final live check) came up in front while the keys kept going to the app in front.
- A toast window, shown with `showInactive`, was on top and above Notepad while Notepad kept the keyboard.
- In both question windows, Enter, Space and Escape pick the default (Deny, Keep).

## Options
| Option | Pros | Cons |
|---|---|---|
| Leave the keys with his app | As R1's box and the toasts: his typing is never interrupted, and an Enter meant for his editor can't deny a start | He answers with the mouse, or clicks into the window first to use its keys |
| Take the keyboard | He can answer with keys at once | Whatever he was typing goes to the window; an Enter meant for his editor picks Deny |

## Recommendation
Leave the keys with his app.

## Choice
The Allow window and the close question show on top without taking the keyboard, as the toasts do. He answers with the mouse, or clicks into the window to use its keys.

## Reason
"Leave the keys with my app".

## Consequences
- Amends 0519.
- The live check: with another app in front receiving keys, each window shows on top and the keys keep going to that app.
- Amended by 0552: the windows go; questions are answered in herdr.
