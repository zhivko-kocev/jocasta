# 0366 Frontier's window opens in Chrome first

- Status: decided
- Kind: product
- Phase/slice: 4/4.1
- Date: 2026-10-09

## Question
Which browser opens Frontier's app window?

## Facts
- 0255 decided Edge app mode, else Chrome or Chromium app mode, else the default browser with the URL printed.
- Chrome is installed at `C:\Program Files\Google\Chrome\Application\chrome.exe`; the agents' browser tool already prefers Chrome (0348).

## Options
| Option | Pros | Cons |
|---|---|---|
| Chrome first, then Edge, then Chromium, then the default browser | His preference | — |
| Edge first, as 0255 | — | He doesn't like Edge |

## Recommendation
None; his request.

## Choice
Chrome app mode first, then Edge, then Chromium, then the default browser with the URL printed.

## Reason
"i dont like edge maybe we can go with chrome?"

## Consequences
- Amends 0255.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
