# 0036 Toolchain

- Status: decided
- Kind: product
- Phase/slice: 0/0.1
- Date: 2026-10-04

## Question
Which tools run, build, test, lint and format the repo?

## Facts
- 2026-10-04: Bun's latest release is 1.4.2 (`npm view bun version`); 1.4.0 is installed on this machine. The Bun pin is decided in 0.2.
- Pi's monorepo (`earendil-works/pi`, root `package.json`): npm workspaces, Biome 2.3.5, `tsc --noEmit`, `node --test`. Pi's binaries are compiled with Bun.

## Options
| Option | Pros | Cons |
|---|---|---|
| Bun for runtime, build and tests; Biome for lint and format | One tool; tests run on the runtime the compiled exe uses | Diverges from Pi's npm and node test setup |
| Node, npm and Vitest for development; Bun only for the compile step | Closer to Pi's dev setup | Tests run on a different runtime than the shipped exe |

## Recommendation
Bun and Biome.

## Choice
Bun for runtime, build and tests; Biome for lint and format.

## Reason
Not given.

## Consequences
`bun install`, `bun run lint`, `bun run format`, `bun test`, `bun run build` (from 0.2).
