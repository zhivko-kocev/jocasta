# 0584 Setup, settings, tests, names and stale files

- Status: v1
- Kind: product, process
- Phase/slice: program
- Vision: V3, V13
- Date: 2026-11-09

## Question
The review found that setup can overwrite a change made while it waited, setup and uninstall rewrite his whole `keybindings.yml`, the suite runs one file serially, jocasta.yaml is validated in six styles, one process has three names, the protocol keeps dead messages, and two tracked files describe the gone companion. What changes?

## Facts
Verified 2026-11-09 by the review (0573), reports SET, TST (TST-3), SHR (SHR-11), DUP (DUP-19, DUP-20, DUP-24) and FE (FE-13).
- Every setup `apply` writes the text made at plan time (`src/setup/setup.ts:267`, :482); a probe saved config.yml between the diff and the `y`, and the save was lost.
- Uninstall and setup's key merge re-serialize `keybindings.yml` (`uninstall.ts:288`), dropping his comments; migration 4 can end "Done" without linking the herdr plugin, and a plain setup never reloads herdr after a herdr.toml change.
- `test/frontier/server/service.test.ts` is 3,953 lines, 77 tests and 202 s of the suite's 278.9 s; `package.json`'s `test` is `bun test`; Bun 1.4.3 has `--parallel`.
- jocasta.yaml: `pollers.github.everyMinutes: 0` errors with no file or line, `gihub:` passes silently, and a bad `frontier.port` becomes 47400 without a word (`server.ts:255`).
- "companion", "service" and "frontier" name the same process: H3's fault `"companion"`, H5's `companionMs`, `companion-feed.jsonl`, `frontier.json`, `frontier.port`, `FrontierServer`.
- `boss.send`, `today.open`, the `boss` and `quiet` broadcasts, `snapshot.boss` and `snapshot.quietHours` are sent or read only by tests; `PRODUCT.md` describes the companion window, and `.impeccable/` (104 tracked files) holds its design mocks.
- On Linux with plain Docker (not Docker Desktop), Graphiti's container can't reach the service's model endpoint, which listens on 127.0.0.1 only (SEC-LOC-10); Windows and Docker Desktop are unaffected.
- `<data>/frontier.json` holds the service's port, API token and pid; the shipped guard denies any command or path containing `frontier.json` (`config/guard.yml:91`) so no agent reads the token. A file renamed to `service.json` would need `*service.json*`, which also catches any project's `service.json` (asked 2026-11-09, during the build).

## Options
| Item | Options (pros / cons) |
|---|---|
| SET-4 stale diff | A. refuse when the file changed · B. re-plan and ask again (more code) · C. keep |
| SET-5 keybindings | A. edit only Jocasta's own key lines · keep |
| SET-15 herdr | note herdr missing, reload after herdr.toml · keep |
| TST-3 parallel tests | A. `--parallel` in `test` and CI (~3× faster) · B. CI only · C. split only |
| Settings (SHR-11, DUP-19) | B / A. unknown keys said once, a bad value turns off its own feature, one port notification · A / B. refuse the file (a working file may stop) · C. file:line only |
| DUP-20 naming | A. code and messages only, data keys kept (no format change mid-study) · B. data keys and file names too, readers accept both (consistent / touches result formats) · C. leave it |
| FE-13 dead messages | A. remove them · B. comments and markers only |
| Stale files | rewrite PRODUCT.md, delete `.impeccable/` · delete both · keep |
| SEC-LOC-10 Linux Docker | say plainly that Graphiti needs Docker Desktop on Linux (no new network surface / no memory there) · a second listener for `/llm/v1` on the Docker bridge (memory works / a listener beyond loopback) |

## Recommendation
SET-4 A; SET-5 A; SET-15 fix; TST-3 A; SHR-11 B with DUP-19 A; DUP-20 A; FE-13 A; rewrite PRODUCT.md and delete `.impeccable/`; SEC-LOC-10 say it plainly.

## Choice
- SET-4 A: each setup change keeps the text it diffed; `apply` re-reads and refuses when the file changed: "jocasta setup: 0003-omp-settings (omp settings Jocasta ships) failed: <home>\.omp\agent\config.yml changed since the diff; run jocasta setup again; the 5 changes after it wait."
- SET-5 A: setup and uninstall edit only their own top-level key lines in `keybindings.yml` (no new dependency); his comments and formatting stay.
- SET-15: setup notes herdr missing instead of "Done" without linking the plugin; after herdr-config applies, it reloads herdr as `--refresh` does ("herdr reloaded its config.").
- TST-3 A: `bun test --parallel` in package.json's `test` script (so `mise run check` gets it) and in CI; service.test.ts is first split into about 8 topic files, after the wait fixes (TST-1, 2, 7).
- Settings (SHR-11 B + DUP-19 A): one typebox schema for jocasta.yaml, validated once with `parseYamlFile`; unknown keys are said once (toast and log) and ignored; a bad value turns off only its own feature, with file:line; a bad `frontier.port` gets one notification: "settings: frontier.port must be a port number from 1 to 65535; using 47400".
  As built: the notification names the file and line as every settings problem does ("with file:line" above), e.g. "settings: jocasta.yaml:7: frontier.port must be a port number from 1 to 65535; using 47400".
- Naming (DUP-20 B): "companion" and "frontier" as names of the service process become "service" in code, messages, data keys and file names, with readers accepting both the old and the new names. The builders list exactly which keys and files changed; `frontier.json` and the result rows' fields keep being read under their old names.
- The token file (asked during the build): `<data>/jocasta-service.json`, not `service.json`; the guard denies `*jocasta-service.json*` and keeps denying `*frontier.json*` (an older build's file). Readers try the new name first, then `frontier.json`.
- FE-13 A: the dead protocol messages (`boss.send`, `today.open`, the `boss` and `quiet` broadcasts, `snapshot.boss`, `snapshot.quietHours`) and their handlers go; stale comments are rewritten; the plan's appendix I is updated (0027).
- Stale files: both `PRODUCT.md` and `.impeccable/` are deleted; git history keeps them.
- SEC-LOC-10: no second listener; on native Linux Docker, Graphiti's health says "Graphiti down (needs Docker Desktop on Linux: the container can't reach Jocasta's model endpoint)" and memory stays off there.

## Reason
"A" for SET-4, SET-5, TST-3, DUP-19 and FE-13; SET-15 ticked; "B" for SHR-11; "B" for DUP-20 ("rename the data keys too, with readers accepting both"); "delete both" for the stale files. SEC-LOC-10, in his words: "maybe a seconde listener for the benchmarking only or just no memory on linux idk but we start the benchmarks on the weekend so it is early i guess"; the benchmarks run on Windows, so the plain wording, with no new listener, carries it out.
For the token file he picked `jocasta-service.json` (the recommendation) over `service.json` (the guard would catch his projects' `service.json` files) and keeping `frontier.json`.

## Consequences
- Differs from the recommendation on DUP-20 (data keys and file names renamed too, not code and messages only) and on the stale files (PRODUCT.md deleted rather than rewritten).
- Amends 0466, 0467, 0468 and 0479 (setup's and uninstall's edits), 0358 and 0387 (settings validation), 0557, 0392 (the companion's product description goes), 0480/0552 (the process's names) and 0572 (CI's test command).
- The plan's appendix I drops the removed protocol messages (`today.open`, `quiet`, `snapshot.quietHours`, `boss`, `boss.send`) and names the renamed keys and files once the builders list them.
