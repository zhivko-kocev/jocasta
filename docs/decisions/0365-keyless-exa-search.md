# 0365 Keyless Exa in `web_search`

- Status: decided
- Kind: product
- Phase/slice: 6/6.9
- Date: 2026-10-09

## Question
He asked whether pi-web-access (https://pi.dev/packages/pi-web-access) would serve web access better. Which way?

## Facts
Checked 2026-10-09.
- pi-web-access 0.37.0 (MIT, 9.6 MB, 9 dependencies including `@modelcontextprotocol/sdk`, `undici` and `unpdf`) adds its own `web_search` and `fetch_content`, about 30 search providers (mostly keyed), GitHub cloning, PDFs, and YouTube and local-video understanding through Gemini, configured in its own `web-search.json`. Its zero-config search is Exa's public MCP endpoint.
- A direct call to Exa's keyless MCP endpoint (`https://mcp.exa.ai/mcp`, tool `web_search_exa`) from this machine returned good results; DuckDuckGo and Mojeek answer with bot checks here (0364).

## Options
| Option | Pros | Cons |
|---|---|---|
| Keyless Exa in Jarvis's own `web_search` | Fixes search with no install | One more outside service gets queries |
| Replace Jarvis's web tools with pi-web-access | Many providers and extras | 9 dependencies, a 0.x package, its own config; removes 0290/0347's tools |
| Keyless Exa now; its extras after v1 | Search fixed today; extras decided later | — |

## Recommendation
Keyless Exa now; pi-web-access's extras after v1.

## Choice
Keyless Exa now, as the first provider after Anthropic's search, before DuckDuckGo, Mojeek and Brave. Mojeek's captcha work from 0364 is dropped. pi-web-access's extras (PDFs, GitHub cloning, video) are grilled as features after v1.

## Reason
Not given.

## Consequences
- Amends 0347's chain and 0364's first item.
- Search queries go to Exa, as they already go to DuckDuckGo and Mojeek.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
