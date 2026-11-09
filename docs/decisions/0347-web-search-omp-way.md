# 0347 web_search the omp way

- Status: decided
- Kind: product
- Phase/slice: 6/6.9 (amends 0290)
- Date: 2026-10-08

## Question
How does `web_search` work, now that Pi's Anthropic provider can't pass server-side search?

## Facts
Verified 2026-10-08.
- Pi 1.1.0's Anthropic provider sends only custom tools and drops `server_tool_use` and `web_search_tool_result` blocks (`pi-ai/dist/api/anthropic-messages.js`, `convertTools` and the stream handler), so the model can't use Anthropic's server-side search inside a turn.
- omp's `web_search` runs a provider chain (`packages/coding-agent/src/web/search/`): grounded providers through the user's model login, including Anthropic's web search tool in a separate Claude Code-shaped request; credential-free scrapers (DuckDuckGo, Mojeek, Startpage); and keyed APIs (Brave, Exa, Tavily and others).
- `pi-web-access` 0.37.0 (MIT, 654K downloads a month, 9.6 MB, 9 dependencies) offers about 35 providers, zero-config search through Exa's MCP, page fetching, GitHub cloning, PDFs and videos; it overlaps `web_fetch`.
- Jarvis's Anthropic subscription shaping is built (0337, 0338).

## Options
| Option | Pros | Cons |
|---|---|---|
| omp's way, written fresh | Uses his subscription; no key needed; small | Ours to maintain |
| Add pi-web-access | Most providers and extras | An install; overlaps `web_fetch`; Exa gets queries by default |
| Both | Extras too | Most to carry |

## Recommendation
omp's way, written fresh.

## Choice
omp's way, written fresh: Anthropic's web search tool in a separate request through Jarvis's subscription shaping first; credential-free fallbacks next; Brave when `BRAVE_API_KEY` is set.

## Reason
"how omp handles web search? or maybe this https://pi.dev/packages/pi-web-access"; then chosen.

## Consequences
- Replaces 0290's Brave-only fallback rule.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
