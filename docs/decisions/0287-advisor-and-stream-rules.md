# 0287 Advisor and stream rules

- Status: decided
- Kind: product
- Phase/slice: 6/6.5
- Date: 2026-10-08

## Question
How do the advisor and TTSR work?

## Facts
Read 2026-10-08 in oh-my-pi (`docs/settings.md:458-467`, `docs/advisor-watchdog.md:26,86-102`, `docs/cli-reference.md:146`): the advisor is off by default; `/advisor on` or `--advisor` turns it on, and then it passively reviews each primary turn on a cadence and injects notes, staying quiet for 3 turns after it interrupts with a concern.
- His usage: `advise` 221 with 113 advisor notes; TTSR 113 injections (appendix A).
- Verified 2026-10-08 in Pi 1.1.0 (`dist/core/extensions/types.d.ts:242,334,819`): `message_update` stream events, `ctx.abort()`, and `sendUserMessage(…, { deliverAs: "steer" })`.
- His first answer took the recommendation and asked how omp triggers the advisor; the follow-up offered omp's automatic shape.

## Options
| Option | Pros | Cons |
|---|---|---|
| An `advise` tool on demand, plus `stream-rules.yaml` | Cheap | — |
| omp-style automatic advisor, off by default, toggled by `/advisor` | His omp habit | Cost per turn while on |
| Both | — | — |

## Recommendation
An on-demand advise tool and stream rules.

## Choice
- Advisor, omp-style: off by default; `/advisor` toggles it per session; while on, `jarvis/strong` reviews each turn and injects notes, quiet for 3 turns after an interrupting concern.
- TTSR: rules in `~/.jarvis/agent/stream-rules.yaml` (a regex, optional file types, a reminder); a match aborts the streamed response, injects the reminder as steering and retries.

```yaml
- match: '\bas any\b'
  in: [ts, tsx]
  remind: "No `as any`; type it properly."
```

## Reason
"the recomended, also does omp call advisor automatically or on demand?"

## Consequences
- Differs from the recommendation (automatic advisor instead of a tool).
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
