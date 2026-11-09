# 0040 Harness entry

- Status: decided
- Kind: product
- Phase/slice: 0/0.2
- Vision: V7
- Date: 2026-10-04

## Question
How does the `jarvis` binary start the harness?

## Facts
- Pi tag `v1.0.1` (github.com/earendil-works/pi), read 2026-10-04: `MainOptions` is only `{ extensionFactories?: InlineExtension[] }` (`packages/coding-agent/src/main.ts`); `main()` prepends Pi's built-in extensions to ours.
- The package's exports map exposes only `.`, `./client`, `./rpc-entry` and `./experimental/plugin`, so `dist/bun/runtime-setup.js` and `dist/bun/restore-sandbox-env.js` can't be imported by package path.

## Options
| Option | Pros | Cons |
|---|---|---|
| `main(args, { extensionFactories })` | Every Pi CLI feature: modes, auth commands, packages, `/reload`, MCP, Codemode, tool_search | No custom auth storage |
| SDK entry on `createAgentSession` and `InteractiveMode` | Accepts a custom `AuthStorage` | CLI wiring rebuilt; MCP, Codemode and tool_search added by hand |
| `main()` plus a build-time patch adding an auth option | Keyring and the full CLI | Counts toward the fork condition |

## Recommendation
`main()`.

## Choice
Pi's `main(args, { extensionFactories })`.

## Reason
Not given.

## Consequences
- `src/entry.ts` copies Pi's Bun bootstrap (`restore-sandbox-env`, `runtime-setup`) with the source path and Pi version in a comment, then calls `main()`.
- Keyring credentials (0.4) need an upstream PR or another route; the fork condition still applies (plan 5.5).
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
