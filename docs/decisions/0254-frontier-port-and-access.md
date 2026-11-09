# 0254 Frontier port, access and single instance

- Status: decided
- Kind: product
- Phase/slice: 4/4.1
- Date: 2026-10-08

## Question
How is the server reached, protected and kept single?

## Facts
- Any web page in a browser can send requests to a localhost server, so the UI needs a secret.

## Options
| Option | Pros | Cons |
|---|---|---|
| 127.0.0.1, a free port, a token in a data-dir file, a token cookie; a second `jarvis` reuses the running server | Closed to other pages and machines | — |
| The same on a fixed port | Bookmarkable | Collisions |

## Recommendation
As recommended.

## Choice
Now (0358): the port is fixed by `frontier.port` in `settings.json`, default 47400, and a free port is used for one launch only when that one is busy. The rest is unchanged.

Originally: The server listens on 127.0.0.1 on a free port chosen at start, and writes `<data>/frontier.json` with `port`, `token` and `pid`. The window opens `http://127.0.0.1:<port>/?t=<token>`; the token becomes a cookie, and requests without it are refused. `jarvis` reads the file, checks the server's health, and opens a window to it, or starts a server if none answers.

## Reason
Not given.

## Consequences
- `frontier.json` is a new file in the data dir (0095).
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
