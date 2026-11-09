# 0512 The review asks for the test command unpiped, after the full run

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-11-01

## Question
Since 2026-11-01 a review's `TESTS: pass` counts only when the test command's exit code is its own (`ownExit`, carrying out 0453): a piped or masked run reads "tests unverified". Does the review prompt ask for an unpiped run, and when?

## Facts
Verified 2026-11-01.
- omp's shell runs without pipefail: `false | cat` exits 0 (omp 18.13.1's `executeBash`).
- The review prompt asks for the tests "as one bash command" (`src/frontier/server/hunk.ts` `reviewPrompt`); arm A3 carries its own copy of that text (`evals/variants/a3.yaml`), so changing the prompt during 0482's runs would split the arm from the product.
- HumanLayer's hook: "Do not pipe test, typecheck, or quality output to head/tail/grep/etc." (https://github.com/humanlayer/humanlayer).

## Options
| Option | Pros | Cons |
|---|---|---|
| Ask for it after the full run | Fewer "tests unverified"; A3 matches the service during the run | Waits for the run |
| Leave the prompt | No change | Piped runs read "tests unverified" |

## Recommendation
Ask for it after the full run.

## Choice
After the full run, the review prompt asks for the test command as one bash command with its output unpiped (to a file if it's long), so its exit code counts.

## Reason
"Unpiped test command in the review prompt".

## Consequences
- Carries out 0453 with 0508's run conditions: arm A3 keeps the pilot's text through the run.
- `test/frontier/server/hunk.test.ts` checks the line once it lands.
