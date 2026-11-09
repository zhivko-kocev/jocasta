# 0175 Self-extension

- Status: decided
- Kind: product
- Phase/slice: 1/1.11
- Date: 2026-10-07

## Question
Is the catalog's self-extension row (0081) in v1?

## Facts
Verified 2026-10-07 in Pi 1.0.1: `/reload` reloads extensions, skills, prompts, themes and context files (`dist/core/slash-commands.js:25`); Pi discovers extensions in `<config dir>/extensions` (`dist/core/resource-loader.js:751-755`).
- Pi's docs ship with Jarvis and `examples/` doesn't (0044, 0171); package installs and `/reload` are kept (0045).
- Model-written code runs with full access; the guard lands in phase 2, and the fixture gate (0026) applies until then.
- No recommendation was marked in the question.

## Options
| Option | Pros | Cons |
|---|---|---|
| v1 | "Build me an extension" works against `~/.jarvis/agent/extensions` and `/reload` | Model-written code runs unless he reads it |
| After v1 | Wait for the guard and approvals | Extensions by hand meanwhile |
| Dropped | Only reviewed code runs | No mods on request |

## Recommendation
None was marked in the question.

## Choice
v1: asked for a feature, Jarvis writes an extension into `~/.jarvis/agent/extensions/`, reading Pi's shipped docs, and loads it with `/reload`.

## Reason
Not given.

## Consequences
- Appendix A's row is v1. Phase 2's guard covers writes into the config dir like any other write.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
