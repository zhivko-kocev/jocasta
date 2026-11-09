# 0384 The program on upstreams

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-13

## Question
When does the move onto the upstreams run?

## Facts
Verified 2026-10-13.
- 0375's program is next at C1 roles, then C2 plan mode, C3 `/pause`, C4 LSP and D code review; all of them exist in omp.
- B1 baselines (2026-10-12, 12 cases × 3): `copilot` 33/36, `anthropic` 33/36; run dir `<data>/evals/runs/2026-10-12T22-24-42-887Z-copilot`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Now, staged, today's Jarvis as fallback | Stops building what omp already has; each stage proven before it replaces anything | 0375 pauses |
| After 0375 | Finishes the measured program first | Builds C1–D on Pi, which omp already covers |
| Harness now, the rest after v1 | Smaller program now | herdr, canvas and review wait |

## Recommendation
Now, staged, today's Jarvis as fallback.

## Choice
Now, in stages: F0 records and plan → F1 omp is the harness → U the UX pass → F2 herdr work surface and companion → F3 whiteboard canvas → F4 hunk review. 0375 pauses and resumes after F4 with E, A6, B2, G1 and H where they still apply. The installed Jarvis stays the fallback until a stage passes and he installs it. 0375's eval keep rule runs and is reported; his word keeps a stage.

## Reason
"Now, staged, today's Jarvis as fallback".

## Consequences
- `AGENTS.md` "Where we are" names the program and its next stage.
- Plan section 6 gains the row "Program on upstreams: F0 F1 U F2 F3 F4".
