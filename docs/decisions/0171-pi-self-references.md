# 0171 Pi's self-references in the prompt

- Status: decided
- Kind: product
- Phase/slice: 1/1.11
- Date: 2026-10-07

## Question
What happens to Pi's prompt text that names pi and routes to `examples/`?

## Facts
Verified 2026-10-07 in Pi 1.0.1:
- The preamble reads "You are an expert coding assistant operating inside pi, a coding agent harness." (`dist/core/system-prompt.js:81`).
- The docs section routes questions about pi, its SDK, extensions, themes, skills and TUI to the README, `docs/` and `examples/` (`:86-93`); Jarvis releases ship `docs/` without `examples/` (0044).
- A `before_agent_start` handler can change prompt sections (`docs/extensions.md:103`).

## Options
| Option | Pros | Cons |
|---|---|---|
| Rename the preamble to Jarvis on pi, and drop the `examples/` lines | Matches the persona (0068); no dead paths | A section edit to re-check on Pi bumps |
| Rename and ship `examples/` | Pi's routing works fully | Reopens 0044 |
| Leave Pi's text | Nothing to do | The model may say it is pi; dead path |

## Recommendation
Rename and drop `examples/`.

## Choice
A Jarvis `before_agent_start` handler sets the preamble to "You are an expert coding assistant operating inside Jarvis, Zhivko's harness built on pi." and removes the `examples/` lines from the docs section, keeping the README and `docs/` lines.

## Reason
Not given.

## Consequences
- Closes the open items on Pi's prompt naming pi and on the `examples/` route.
- The self-extension row (0081) is asked next without `examples/`.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
