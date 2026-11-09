# 0378 Jarvis everywhere

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Date: 2026-10-10

## Question
Where does Pi's name or Pi's machinery still show through Jarvis, and what is changed?

## Facts
Verified 2026-10-10.
- Pi's dist hard-codes "restart pi" (`interactive-mode.js:3436`), the crash hint "run /bug" (`:1648`) and "Update pi" (`package-manager-cli.js:296-313`).
- `/bug` uploads to Pi's `/v1/bug-reports`; it asks before uploading and names the host.
- Startup details list all 28 `<inline:…>` extensions. Pi's `quietStartup: true` hides the header and the details; `"header"` keeps the header and hides the details. A header set with `setHeader` is shown either way.
- Pi titles the terminal `${APP_TITLE} - ${sessionName} - ${folder}` and re-applies it whenever the session info changes and on every resize on Windows. `APP_TITLE` is `jarvis` in the build. `pi.setSessionName()` exists.

## Options
| Option | Pros | Cons |
|---|---|---|
| Seed `quietStartup: "header"`, name the session after the thread, document Pi's own words | No patching of Pi's dist; the title survives Pi's re-titling | Hard-coded "pi" texts stay |
| Patch Pi's dist texts | Pi's name gone everywhere | A patch to re-check at every Pi bump |

## Recommendation
Seed, name, document.

## Choice
- `quietStartup: "header"` is seeded: the extension list is hidden and Jarvis's header stays.
- On session start in the TUI, a thread's session is named after its title, so the terminal reads `jarvis - <thread title> - <folder>`.
- `/bug` stays. The README notes Pi's own words: what `/bug` uploads or exports, the unpatched "restart pi", "start pi", "pi bug" and "Update pi" texts, and adding `"quietStartup": "header"` by hand to an existing settings.json.

## Reason
Branding: "whatever makes sense", with `/bug` kept.

## Consequences
- Amends 0060: the header stays as the one startup line; and 0056: the seed gains `quietStartup: "header"`.
- 0363's startup-list naming shows only with `--verbose` now that the list is hidden by default.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
