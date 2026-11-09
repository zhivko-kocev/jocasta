# 0491 `jocasta` exits 0 after Jocasta's own stop

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
Bare `jocasta` exits with herdr's client's code, and herdr's client exits 1 when its server is stopped. So after every normal Stop everything `jocasta` returns 1, while appendix I also says `jocasta` exits 0 on success. Which holds?

## Facts
Verified 2026-10-24 in the campaign's sandbox runs (0481): `jocasta.exe` exited 1 after each normal Stop everything (the UI walkthrough and both closes of the beta day: "exited 1 after 2659.0 s", "after 427.7 s").
- Plan appendix I: "`jocasta` exits 0 on success, 1 on failure and 2 on an invocation error (0387); bare `jocasta` exits with herdr's client's code."

## Options
| Option | Pros | Cons |
|---|---|---|
| 0 after Jocasta's own stop | A normal close reads as success to shells and scripts; matches "0 on success" | One more case in the exit-code rule |
| Keep herdr's code | No change | Every normal close looks like a failure |

## Recommendation
0 after Jocasta's own stop.

## Choice
When the companion stopped herdr's server itself (Stop everything, or Keep's later stop), bare `jocasta` exits 0. Any other exit of herdr's client keeps herdr's code.

## Reason
"0 after Jocasta's own stop".

## Consequences
- Amends 0387's exit codes as appendix I states them; appendix I's exit-code paragraph names the new case.
