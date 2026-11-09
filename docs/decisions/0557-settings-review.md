# 0557 The settings review: the app-only settings go, everything else stays

- Status: v1
- Kind: product
- Phase/slice: program
- Vision: V11
- Date: 2026-11-09

## Question
He asked to "see settings ui settings everything" after the herdr pass (0548). With no app (0552), which settings change?

## Facts
Verified 2026-11-09 against `src/shared/settings.ts`, `src/shared/pollers/settings.ts`, `src/app/display.ts`, `src/app/autostart.ts`, `config/omp-config.yml`, `config/boss.yml`, `config/guard.yml`, the per-project overlay (`overlayText`, `service.ts:264`), plan appendix I and omp 18.18.2's `omp://settings.md`.
- `jocasta.yaml` keys: `frontier.port` (47400), `frontier.autostart` (off, 0549), `frontier.quietHours`, `frontier.dashboard.display` (which display holds the dashboard), `dashboard.weather`, `pollers.github|azureDevOps|calendar|news`, `graph.main|small`.
- `frontier.dashboard.display` places the companion window; `frontier.autostart` starts the app at logon. 0552 removes the app.
- `lsp.shared` (omp default true): "Each connection gets its own server. A released server stays warm for up to 5 minutes … only while another omp process keeps the project's mux running"; it falls back to private servers.
- The shipped omp settings, the boss overlay, the guard, the per-project overlay and setup's omp files (0550) are all registered in 18.18.2.

## Options
| Item | Options (pros / cons) |
|---|---|
| App-only settings | Drop both · Drop the display, keep autostart as a terminal running `jocasta` at logon |
| Everything else | Keep all as shipped · Change named rows |

## Recommendation
Drop both; keep everything else.

## Choice
- `frontier.dashboard.display` and `frontier.autostart` go; a `jocasta.yaml` that still sets them is told they're gone, and setup removes the logon Run value it wrote. He starts `jocasta` himself.
- Kept unchanged: the shipped omp settings (theme, memory, compaction, privacy, collab off outside 0536's relay, secrets, LSP diagnostics), the boss overlay and its status line, the guard, the per-project overlay, setup's omp files, `lsp.shared` on omp's default, the pollers and their intervals and feeds, `frontier.port`, `frontier.quietHours`, `dashboard.weather`, `graph.main|small`, and the `frontier.*` key names.

## Reason
"Drop both"; "Yes, keep all".

## Consequences
- Amends 0549 (autostart goes), 0367 (the Run value) and 0428, 0439 (the dashboard's display setting).
- Plan appendix I loses the two keys when the build lands.
