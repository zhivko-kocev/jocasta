# 0364 Search through the headless browser; more browser actions

- Status: decided
- Kind: product
- Phase/slice: 6/6.8, 6.9
- Date: 2026-10-09

## Question
With no Anthropic login, `web_search` fails on this machine. What fixes it? And which browser additions does he want?

## Facts
Checked 2026-10-09.
- From this machine DuckDuckGo answers with a bot-check page (HTTP 202) and Mojeek with an ALTCHA page (HTTP 200) or 403, so the chain of 0347 fails unless `BRAVE_API_KEY` is set.
- omp (`packages/coding-agent/src/web/search/providers/mojeek.ts`) loads Mojeek in its browser and completes the ALTCHA proof-of-work checkbox, waiting up to 45 s for the verified redirect. For DuckDuckGo, Google, Ecosia and Startpage challenges it reports the block and moves on; it doesn't solve them.
- 0348 gave the browser five actions on one 1280×800 page; 0237 keeps it out of chat-only threads and plan mode.

## Options
| Option | Pros | Cons |
|---|---|---|
| Brave with `BRAVE_API_KEY` | Reliable; no code | A key |
| Mojeek in the headless browser, omp's way | No key | Breaks when the site changes |
| Another keyed provider (Tavily, Exa) | Reliable | A key and a new provider |

Browser additions, ticked from: read-only use in chat-only and plan mode; a scroll action; several tabs.

## Recommendation
Brave with `BRAVE_API_KEY`.

## Choice
- Now (0365): dropped; keyless Exa fixes search instead. Originally: when Mojeek answers with its ALTCHA page, `web_search` loads it in the headless browser and completes the proof-of-work checkbox the way a browser does, as omp does, before falling back to Brave. Image challenges and other providers' bot walls are reported, not solved.
- The browser is allowed read-only in chat-only threads and plan mode: open, screenshot and evaluate, but not click or type.
- A scroll action.
- Several tabs per thread.

## Reason
Not given.

## Consequences
- Amends 0347 (search chain), 0348 (actions) and 0237 (chat-only tools).
- Differs from the recommendation.
