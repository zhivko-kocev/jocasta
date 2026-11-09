# 0435 Upstream bumps for the polish round

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-10-15

## Question
The polish round checked every pinned upstream against its latest release. Which bumps does it take?

## Facts
Verified 2026-10-15.
- Bun 1.4.3 (2026-10-10) over the pinned 1.4.2: 67–89% less idle CPU (the 1 s GC timer no longer keeps running), 6–11% less idle memory, `fetch`, `Bun.serve` and named-pipe leak fixes, a Windows `terminal.write()` leak fix. It runs `jarvis.exe` (the CLI and one `jarvis mcp` per omp pane), the tests and the build; the companion runs on Electrobun's own Bun 1.4.0.
- react and react-dom 19.3.1 (2026-10-14) over 19.3.0, a patch; `@types/react` and `@types/react-dom` 19.3.0 are the latest.
- typebox 1.3.37 (2026-10-12) over 1.3.27, a patch series.
- herdr 0.9.5 (2026-10-14) over 0.9.4: a hotfix (prefix keys sent as text, Windows plugin panes, non-ASCII user folders); no socket API change listed. `herdr.ts` accepts any 0.9.x.
- omp 18.11.0 (2026-10-14) over his installed 18.10.4, a minor; plan 5.5's omp seam list had not been re-checked against it.
- Electrobun 2.0.3 and hunk 0.24.0 exist only as betas; electrobun 2.0.2, typescript 7.0.2, biome 2.5.15, monaco 0.57.0, sherpa-onnx-node 1.13.8, node-ical 0.27.3, the React Compiler 1.0.0, @babel/core 8.0.7, hunk 0.23.0, whiteboard v0.2.4 and graphiti-core 0.30.2 are at their latest stable.

## Options
| Option | Pros | Cons |
|---|---|---|
| All stable patches + omp re-check | Idle CPU and memory fixes in every `jarvis mcp`; seams re-checked | One more pin to move |
| Bun only | Smallest change | React, typebox and herdr fixes left out |
| Nothing | No risk from bumps | Known leaks stay |

## Recommendation
All stable patches + omp re-check.

## Choice
Bun 1.4.3 (`packageManager`, `@types/bun` 1.4.3, CI and release `bun-version`, installed here with `bun upgrade`), react and react-dom 19.3.1 (`@types/react*` stay 19.3.0, the latest), typebox 1.3.37, herdr 0.9.5 (installed here with `herdr update`, no `--handoff`). omp 18.11.0 is re-checked against plan 5.5's omp list from its npm package (not installed; his omp stays his) and `docs/research/omp-adaptation.md` gains an 18.10.3 → 18.11.0 settings delta for his `/settings` pass. Electrobun 2.0.3 and hunk 0.24.0 stay out (betas).

## Reason
"All stable patches + omp re-check".

## Consequences
- Amends 0042 (the Bun pin) and 0385 (herdr 0.9.4).
- A Bun regression found in the round pins back to 1.4.2 and is written here.
