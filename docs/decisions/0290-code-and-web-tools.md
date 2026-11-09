# 0290 Code and web tools

- Status: decided
- Kind: product
- Phase/slice: 6/6.9
- Date: 2026-10-08

## Question
Which code and web tools does Jarvis add?

## Facts
- Verified 2026-10-08: Pi has no web search or fetch tools (its docs mention none); `gh` and `az` are on PATH; ast-grep (`sg`) is not. His usage: web_search 756, `https://` reads 911, conflict 75, pr and issue 6, ast_edit 6 (appendix A).

## Options
| Option | Pros | Cons |
|---|---|---|
| `web_fetch`, `web_search` by a rule, `conflict://`, `pr://` and `issue://` through `gh` and `az`, ast-grep when `sg` is on PATH | Covers his use | — |
| The same without ast-grep | Smaller | — |
| The same without pr and issue | Smaller | — |

## Recommendation
As recommended.

## Choice
Now (0347): `web_search` runs omp's chain (Anthropic web search on his subscription, then DuckDuckGo and Mojeek, then Brave with a key). Now (0307): ast-grep is after v1; the rest stands.

Originally: - `web_fetch`: HTTP plus HTML-to-markdown, through the guard's net rules.
- `web_search`: the model's server-side search if Pi's Anthropic provider passes it, checked at build; otherwise the Brave Search API with his key.
- `conflict://` reads merge conflicts; `pr://` and `issue://` go through `gh` (GitHub) and `az` (Azure DevOps).
- ast-grep only when `sg` is on PATH.

## Reason
Not given.

## Consequences
- Slice 6.9 is decided, and with it phase 6.
- Amended by 0307: ast-grep after v1.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
