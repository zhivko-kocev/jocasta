# 0044 Packaging

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
What ships in a release archive?

## Facts
- Pi tag `v1.0.1` (github.com/earendil-works/pi), read 2026-10-04: `scripts/build-binaries.sh` compiles `dist/bun/cli.js` plus two workers, then copies `package.json`, README, CHANGELOG, photon's `photon_rs_bg.wasm`, `theme/*.json`, `assets/`, `export-html/`, `docs/`, `examples/` and pi-tui's native helpers under `native/<os>/prebuilds/<platform>/`.
- A compiled binary reads `package.json` from the exe's folder (`getPackageDir()` returns `dirname(process.execPath)`), and with it `piConfig`, the app name and the version.
- Dev runs through `bun src/entry.ts` read Pi's own `package.json`, so only the compiled build carries the Jarvis name.
- With no models, Pi's first-run message links to `<install>/docs/providers.md` and `docs/models.md` (found in the 0.2 smoke).
- The npm package ships the compiled workers `dist/extensions/codemode/worker.js` and `dist/utils/image-resize-worker.js`.

## Options
| Option | Pros | Cons |
|---|---|---|
| Pi's layout without Pi's `docs/` and `examples/` | Every Pi feature works under the Jarvis name | Not a single file |
| Pi's layout with `docs/` and `examples/` | Pi's in-app doc links resolve | Bigger; Pi's docs describe `pi` |
| The exe alone | One file | Runs as `pi` with `~/.pi`; themes, images and export break |

## Recommendation
Pi's layout without docs and examples. Re-asked on 2026-10-04 after the smoke found the first-run links broken: ship Pi's whole `docs/`.

## Choice
A zip (windows) or tar.gz per target holding `jarvis[.exe]`, `package.json` with `piConfig`, `native/`, `theme/`, `assets/`, `export-html/`, `photon_rs_bg.wasm`, README, CHANGELOG and Pi's `docs/` (2.8 MB). Pi's `examples/` is left out.

## Reason
Not given.

## Consequences
- Pi's doc links resolve; the docs describe `pi`, not `jarvis`.
- Links into Pi's examples don't resolve.
- Edited in place on 2026-10-04: the first answer left `docs/` out.
- Amended by 0386: Pi's archive layout is gone with Pi; the archive holds `jarvis.exe`, `config/`, `app/`, `node/`, `whiteboard/`, `hunk/` and `install.json`.
