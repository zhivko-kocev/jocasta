# 0332 Phase 4 as-built texts and test scripts

- Status: decided
- Kind: product
- Phase/slice: 4/4.1–4.12
- Date: 2026-10-08

## Question
Are phase 4's as-built visible choices accepted, and does offline testing get per-session scripts?

## Facts
- No record set these; each was explained at his request. One fake script feeds every in-process session; tests swap the provider per session.

## Options
| Option | Pros | Cons |
|---|---|---|
| As built; offline scripts stay tests-only | Lean | No scripted demo of the compiled app |
| Add `JARVIS_FAKE_SCRIPT_DIR` | Scripted demos | A new env var |

## Recommendation
As built; tests only.

## Choice
- A boss-started thread's title is the brief's first line, up to 60 characters; a scout brief starts with `Shape: scout. Write a report; change no code.`
- Toasts: `<thread> needs you: …` and `<thread>: done — <summary>`; 6 seconds at the bottom right. Now (0362): a needs-you toast closes once its question is answered anywhere. Originally: it stayed until clicked.
- Digest: Needs you (questions and interrupted threads), Landed (done, failed, cancelled), Underway (running), Next (queued).
- Blocked threads and a project thread he drives by hand count against the running limit.
- UI labels: ◌ queued, ○ starting, ✓ done, ✗ failed, – cancelled, ‖ interrupted, · idle; tabs `project · title` and `boss`; replies marked `jarvis ›`; Enter sends, Esc stops the run; HUD `N% left`, `digest`, `Quit`; `↓N` commits behind.
- Dictated text goes in at the cursor; a Space tap in an empty input also starts recording.
- No `JARVIS_FAKE_SCRIPT_DIR`: per-session scripts stay inside the tests. Now (0376): the fake provider itself is a test fixture, and no product code reads a `JARVIS_FAKE_*` variable.

## Reason
"explain the points"; on scripts: "explain"

## Consequences
- None.
- Amended by 0395: the look is re-decided in the UX pass before F2.
