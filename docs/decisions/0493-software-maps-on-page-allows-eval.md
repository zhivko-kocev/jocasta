# 0493 Software maps on: the page allows eval

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-10-24

## Question
Whiteboard's software maps (architecture diagrams with routed connectors) need libavoid, whose WebAssembly glue builds functions from strings, which the companion page's Content-Security-Policy blocks. Leave software maps off, allow eval, or rebuild the library without eval?

## Facts
Verified 2026-10-24 in the campaign's canvas fixes (0481).
- The page's policy is `script-src 'self' 'wasm-unsafe-eval'` (`src/frontier/server/server.ts`), strict because WebView2 runs with web security off (0368).
- libavoid's embind glue calls `Function` on strings through a Trusted Types policy (`reviewLibavoid`); the policy blocks it, so whiteboard's software-map routing can't start. Jocasta's canvas bridge reports `softwareMapEnabled: false` (`src/frontier/ui/whiteboard/bridge.ts`), so the feature is off; every other view renders.
- In headless Edge 156, `'trusted-types-eval'` with `require-trusted-types-for 'script'` let a plain `Function("return 2")` run too, so it is no narrower than `'unsafe-eval'`.
- The page holds the HttpOnly cookie that lets it call Jocasta's service; eval makes injected text easier to turn into running code if any ever reaches the page.
- `@mr_mint/elkjs-libavoid` rebuilt with emscripten's `-sDYNAMIC_EXECUTION=0` needs no eval, at the cost of emscripten and a new build step.

## Options
| Option | Pros | Cons |
|---|---|---|
| Leave software maps off for v1 | The policy stays strict | No software maps |
| Allow eval | Software maps work now | The page can run code built from strings |
| Rebuild the library without eval | Software maps with the policy kept | emscripten installed and a new build step |

## Recommendation
Leave software maps off for v1.

## Choice
The page's policy adds `'unsafe-eval'` to `script-src` (still no inline scripts and no other origin), and the canvas reports software maps enabled, so agents can draw them.

## Reason
"Allow eval" (differs from the recommendation).

## Consequences
- Amends 0368 (the page's policy no longer blocks eval).
- README's security notes say the page allows eval for whiteboard's software maps; the rebuild without eval stays open for after v1.
