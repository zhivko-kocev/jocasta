# 0053 Bun bootstrap import

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Date: 2026-10-04

## Question
How does `src/entry.ts` run Pi's Bun bootstrap when the exports map hides it?

## Facts
- Installed `@earendil-works/pi-coding-agent` 1.0.1, 2026-10-04: Pi's Bun entry `dist/bun/cli.js` imports `sandbox-env-setup.js`, then `runtime-setup.js`, then `../cli.js`.
- `runtime-setup.js` sets `process.title`, registers Bun OAuth flows and the Bedrock provider (public `@earendil-works/pi-ai` exports), and calls `setEmbeddedQuickJSWasmPath(quickjsWasmPath)` from `../config.js`.
- `setEmbeddedQuickJSWasmPath` is not exported from the package root, and the exports map blocks `dist/bun/*`. Without it Codemode resolves `quickjs-wasi/quickjs.wasm` through `createRequire`, which fails inside a compiled exe.
- The plan's build sketch said to copy the files if the exports map blocks them; a copy can't make that call.

## Options
| Option | Pros | Cons |
|---|---|---|
| Import Pi's two bootstrap files by file path from the installed package | Exact Pi behavior; same module instances `main()` uses; no copy to drift | Undocumented internal path, re-checked at each Pi bump |
| The same, plus an upstream PR exporting a Bun bootstrap | The path import goes away when merged | A PR to maintain |
| Copy the bootstrap without the QuickJS step | Public API only | Codemode broken in compiled builds, against 0046 |

## Recommendation
File-path import. Bends plan section 7's seams rule; recorded as a fork-condition item in plan 5.5.

## Choice
`src/entry.ts` imports `dist/bun/sandbox-env-setup.js` and `dist/bun/runtime-setup.js` from the installed `@earendil-works/pi-coding-agent` by file path, before importing `main`.

## Reason
Not given.

## Consequences
- The build fails loudly if a Pi bump moves the files.
- Plan 5.5 lists this as a non-documented seam that a Pi bump re-checks.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
