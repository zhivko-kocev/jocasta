# 0283 Model roles and fallbacks

- Status: decided
- Kind: product
- Phase/slice: 6/6.2
- Date: 2026-10-08

## Question
How are model roles and fallbacks built and configured?

## Facts
Verified 2026-10-08 in Pi 1.1.0 (`docs/virtual-models.md`): an extension registers a virtual model whose `route()` picks a physical model and thinking level per request; it appears in `/model` and `--model`; the footer shows the routed model; `/session` lists cost per physical model; on `retry`, `route()` sees the failed request.
- 0268 put `frontier.maxRunningThreads` in `settings.json`. The strict profile has no model fallback (0186).

## Options
| Option | Pros | Cons |
|---|---|---|
| Virtual models, with `roles` in `settings.json` | No new file; Pi shows the routed model | — |
| Virtual models, with a `models.yaml` | Jarvis's YAML | A new file |
| Jarvis's own resolver | — | Rebuilds what Pi has |

## Recommendation
Virtual models with `roles` in settings.json.

## Choice
Now (0377): each role lists GitHub Copilot first, then Anthropic; account rotation is dropped and the in-flight cap stays; the rest stands.

Earlier (0311): the in-flight cap and account rotation are v1 (0311).

Originally: Roles are Pi virtual models `jarvis/default`, `jarvis/cheap` and `jarvis/strong`. Each is an ordered list under `roles` in `settings.json`; the first model with credentials and no recent rate-limit error answers, and a failed request retries on the next. The strict profile turns fallback off.

```json
"roles": {
  "default": ["anthropic/claude-opus-5-5"],
  "cheap":   ["anthropic/claude-haiku-5-5", "anthropic/claude-sonnet-5"],
  "strong":  ["anthropic/claude-opus-5-5"]
}
```

## Reason
Not given.

## Consequences
- The boss's model (0264) becomes `jarvis/default` at thinking `low`.
- Appendix G's in-flight cap and account switching are after v1 with the pruning pass.
- Amended by 0311: the in-flight cap and account rotation are v1.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
