# 0255 Browser fallback

- Status: decided
- Kind: product
- Phase/slice: 4/4.1
- Date: 2026-10-08

## Question
What opens the UI when Edge isn't installed?

## Facts
- Edge is installed on this machine (0251); Linux and macOS builds exist (0043).

## Options
| Option | Pros | Cons |
|---|---|---|
| Chrome or Chromium app mode, else the default browser with the URL printed | Always opens something | — |
| The default browser only | Simplest | Browser bars |

## Recommendation
Chrome or Chromium, else the default browser.

## Choice
Now (0366): Chrome app mode first, then Edge, then Chromium, then the default browser with the URL printed.

Originally: Edge app mode, else Chrome or Chromium app mode, else the default browser in a normal window, with the URL printed.

## Reason
Not given.

## Consequences
- None.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
