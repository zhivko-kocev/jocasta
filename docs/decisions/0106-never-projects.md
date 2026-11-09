# 0106 Never-projects

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
Which folders can never be projects?

## Facts
- Plan 1.2 (v4 F0 slice 4): home and filesystem roots are never projects.
- Asked as a tick-list (0089); ticked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Home and filesystem or drive roots are never projects; `project add` refuses them with a message | A stray launch in the home dir never makes a giant project | — |
| No rule | — | Home and `D:\` could become projects |

## Recommendation
Home and drive roots are never projects.

## Choice
The home dir and filesystem or drive roots (`C:\`, `D:\`, `/`) are never projects; `project add` on one is refused with a message.

## Reason
Not given.

## Consequences
- The scan skips them; 1.4's item 11 decides what Jarvis does when opened there.
