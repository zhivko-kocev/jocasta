# 0325 How billion-context's proxy ships

- Status: decided
- Kind: product
- Phase/slice: 3/3.4, 0.2
- Date: 2026-10-08

## Question
How does billion-context's proxy run from a release?

## Facts
- Building 3.4 found that billion-context's proxy runs as a separate script (`node dist/index.js start`), so it can't run from inside `jarvis.exe`, and it needs a JS runtime. It works on Pi 1.1.0 when model traffic is real HTTP; Pi's own compaction stands down while the proxy carries the conversation.
- Checked 2026-10-08: `jarvis.exe` runs as Bun with `BUN_BE_BUN=1`. Releases already carry Pi's `docs/` next to the exe (0044).
- The proxy checks npm for release notes every 3 minutes without installing (no switch) and patches `fetch` for the whole process.

## Options
| Option | Pros | Cons |
|---|---|---|
| Ship its files next to `jarvis.exe`; Jarvis starts the proxy with `jarvis.exe` in Bun mode | No Node needed | A folder in the release |
| The same, requiring Node on PATH | billion-context's own way | One more requirement |
| Drop billion-context | No proxy | Reverses 0323 |

## Recommendation
Ship next to jarvis.exe, Bun mode.

## Choice
Releases carry `billion-context/` (its `dist/` and `package.json`, pinned 0.1.188) next to `jarvis.exe`. Jarvis starts one proxy per process itself, running `jarvis.exe` with `BUN_BE_BUN=1` set for that child only (`bun` in development), and the plugin attaches to it.

## Reason
Not given.

## Consequences
- Amends 0323: "compiled in" becomes "shipped next to the exe".
- The phase 3 exit smoke runs on the compiled exe too.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
