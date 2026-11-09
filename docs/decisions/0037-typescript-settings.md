# 0037 TypeScript settings

- Status: decided
- Kind: product
- Phase/slice: 0/0.1
- Date: 2026-10-04

## Question
How strict is TypeScript, and is it type-checked?

## Facts
Pi's repo runs `tsc --noEmit` in its `check` script. Bun strips types without checking them.

## Options
| Option | Pros | Cons |
|---|---|---|
| Strict, ESM, `tsc --noEmit` in CI | Catches errors against Pi's typings | A `typescript` dev dependency and a check step |
| Bun's type stripping, no type check | Nothing extra to run | Type errors against Pi's API show up only at runtime |

## Recommendation
Strict, ESM, `tsc --noEmit`.

## Choice
Strict, ESM, `tsc --noEmit` in CI; `bun run typecheck` locally.

## Reason
Not given.

## Consequences
`tsconfig.json` is strict and emits nothing; `bun run typecheck` runs `tsc --noEmit`.
