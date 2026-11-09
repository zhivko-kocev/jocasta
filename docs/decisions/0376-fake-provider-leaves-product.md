# 0376 The fake provider leaves the product

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-10

## Question
Does the scripted fake provider stay compiled into Jarvis, or move into the tests?

## Facts
Verified 2026-10-10.
- `src/harness/fake-provider/{index,script,yaml}.ts` is registered in `src/harness/index.ts` and is active when `JARVIS_FAKE_SCRIPT` names a script.
- Product code that knows about it: `src/frontier/server/host.ts` skips the boss's role model under `JARVIS_FAKE_SCRIPT`; `src/harness/subagents/child.ts` maps `JARVIS_FAKE_SUBAGENT_SCRIPT` to a child's `JARVIS_FAKE_SCRIPT`.
- Its users are tests and evals only. `yaml.ts` is a general YAML parser also used by the guard, the stream rules and the eval runner.
- Pi auto-loads `<agentDir>/extensions/*.ts` and `*/index.ts`, takes `-e/--extension <path>`, and reads an `extensions` list from settings.

## Options
| Option | Pros | Cons |
|---|---|---|
| A test fixture loaded as a Pi extension | The product carries no test-only provider or env vars | Spawned tests install a stub extension into their config |
| Keep it compiled in | No test changes | A shipped provider and two env vars that only tests use |

## Recommendation
A test fixture.

## Choice
- `yaml.ts` moves to `src/shared/yaml.ts`.
- The faux provider becomes `test/fixtures/scripted-model.ts`: the same logic and script format (0162), read from `JARVIS_FAKE_SCRIPT`. After reading its own script it hands `JARVIS_FAKE_SUBAGENT_SCRIPT`, when set, to its children as their `JARVIS_FAKE_SCRIPT`.
- Spawned runs load it through a stub, `<config dir>/extensions/scripted-model.ts`, that re-exports the fixture; in-process launchers pass it inline. A process loads it one way, never both.
- `src/harness/fake-provider/` is deleted; the boss always selects `jarvis/default`; `child.ts` no longer maps the subagent script.

## Reason
His direction: `fake-provider` leaves the product.

## Consequences
- Amends 0161, 0162, 0168, 0169, 0317 and 0332.
- `test/evals/fake-provider.test.ts` (12 tests) is deleted.
- `JARVIS_FAKE_SCRIPT` and `JARVIS_FAKE_SUBAGENT_SCRIPT` are read only by the fixture (tests and evals only, appendix I).
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
