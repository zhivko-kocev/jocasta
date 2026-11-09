# 0328 Frontier UI stack

- Status: decided
- Kind: product
- Phase/slice: 4/4.1, 4.3
- Date: 2026-10-08

## Question
Which UI library does Frontier's web UI use?

## Facts
- Checked 2026-10-08: React 19.3.0; @xyflow/react 12.12.0 (peer react >=17); Preact 11.0.1. React Compiler 1.0 has been stable since October 2025 and memoizes automatically at build time. Bun 1.4.2 serves, bundles and has WebSockets built in. whiteboard uses React 19 with @xyflow/react and elkjs (0295).
- He asked whether React is the best choice and whether React 19 fixes the old slowness; answered: needless re-renders were the cause and the compiler handles them; streaming tokens append to the DOM directly.

## Options
| Option | Pros | Cons |
|---|---|---|
| React 19 with React Compiler | React Flow native for phase 7; largest ecosystem | ~45 KB |
| Preact with signals | ~5 KB | React Flow through compat, unverified |
| No framework | No dependencies | More hand code; phase 7 needs another renderer |

## Recommendation
React 19 with React Compiler.

## Choice
React 19 with React Compiler (build-time), bundled by Bun; streaming tokens append to the DOM directly. New dependencies: `react`, `react-dom`, `babel-plugin-react-compiler`, pinned exactly.

## Reason
"is the react the best choice here and does react 19 solve the old react slow problems"

## Consequences
- Phase 7's renderer can use @xyflow/react.
