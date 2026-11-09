# 0060 Startup header

- Status: decided
- Kind: product
- Phase/slice: 0/0.3
- Date: 2026-10-04

## Question
What does the TUI show at the top on start?

## Facts
- Pi 1.0.1 `interactive-mode.js` draws Pi's logo and the hard-coded line "Pi can explain its own features and look up its docs. Ask it how to use or extend Pi."; `piConfig.name` doesn't change it.
- Extensions replace the header with the documented `ctx.ui.setHeader()` (Pi's `examples/extensions/custom-header.ts`); `keyHint`, `keyText` and `rawKeyHint` are exported.
- The `quietStartup` setting drops the header.

## Options
| Option | Pros | Cons |
|---|---|---|
| A Jarvis header from a compiled-in extension through `setHeader()` | Jarvis's name; documented seam | Pi's expandable startup help goes away |
| `quietStartup: true` in the seeded settings | No code | No header or hints |
| Keep Pi's | Nothing to do | Says it's Pi |

## Recommendation
The Jarvis header.

## Choice
Now (0378): the header stays as the one startup line: the seeded `quietStartup: "header"` hides Pi's extension list, and the terminal title names the thread; the rest stands.

Originally: A compiled-in extension sets the header in the TUI:

```
jarvis v0.1.0
esc interrupt · ctrl+c/ctrl+d clear/exit · / commands · ! bash
```

The keys follow Zhivko's keybindings.

## Reason
Not given.

## Consequences
- `src/harness/header/` is the first compiled-in extension.
- Whether the model calls itself Pi is decided with the system prompt (1.11) and the persona (0.6).
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
