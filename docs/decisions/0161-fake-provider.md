# 0161 Fake provider

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
What provides scripted model responses for offline smokes?

## Facts
Verified 2026-10-07: `@earendil-works/pi-ai` 1.0.1 (installed, MIT) exports `fauxProvider()`, `fauxText`, `fauxToolCall`, `fauxAssistantMessage`, and simulated usage and cache reads (0084). Appendix I claims `JARVIS_FAKE_SCRIPT`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's faux provider, registered by a Jarvis extension and fed by a script file | No provider code of Jarvis's own; real message types | Tied to pi-ai's faux API |
| A provider of Jarvis's own | Jarvis's format | Re-implements Pi |
| A fake HTTP server | Exercises the HTTP path | Most code |

## Recommendation
Pi's faux provider.

## Choice
Now (0376): the faux provider is the scripted-model test fixture (`test/fixtures/scripted-model.ts`), loaded as a Pi extension by tests and evals when `JARVIS_FAKE_SCRIPT` names a script; the product has none.

Originally: Pi's faux provider, registered by a Jarvis extension when `JARVIS_FAKE_SCRIPT` names a script file.

## Reason
Not given.

## Consequences
- The script file's format is a separate decision in 1.10 (0005); the question's preview was illustrative only.
- A Pi bump re-checks the faux API.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
