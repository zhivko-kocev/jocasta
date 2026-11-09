# 0537 Jocasta runs omp threads only; opencode and Claude Code are benchmarked bare

- Status: decided
- Kind: product
- Phase/slice: program
- Vision: V7, V8
- Date: 2026-11-08

## Question
The brainstorm proposed drivers for omp and opencode. Which harnesses does the harness-OS layer (0532) run, and which are benchmarked?

## Facts
Verified 2026-11-08.
- opencode 1.18.22 can be driven with its TUI in a pane (`opencode serve` plus `opencode attach`, `prompt_async`, `/event`), has structured output and cost per message, loads MCP tool definitions all at once, and has reported Windows issues (shell PATH cut to System32, a server hang on the default port).
- omp's approvals are the guard for every repo (0391); opencode has only its own permission rules.
- 0386: whole omp, unforked, is the harness.

## Options
| Option | Pros | Cons |
|---|---|---|
| omp only; opencode and Claude Code bare in the benchmark | 0386 and 0391 stand; no driver for a harness the product doesn't run; competitors compared as their makers ship them | The benchmark can't show the layer helping another harness |
| omp only; opencode also with the layer in one arm | Tests whether the environment helps across harnesses | Builds a driver no product uses |

## Recommendation
omp only; opencode and Claude Code bare.

## Choice
The product and its harness-OS layer run omp threads only. opencode and Claude Code are benchmark arms as whole harnesses, bare, for performance. The ledger (K1) reads all three harnesses' records for the benchmark.

opencode's arm (A7, 0543) runs on an Anthropic API key, its only Anthropic sign-in (0506), with the other arms' model, in the VM only, for that arm only, with a $150 spend limit on the key; it is billed per token (about $48–100 for 90 trials at the pilot's median).

## Reason
"maybe we only do omp panes, we benchmark against opencode as whole harness and claude code as harness only for the performance"; then "omp only; others bare"; told that opencode's Anthropic sign-in takes only an API key (0506) and that 0482 allows no API keys: "API key, capped".

## Consequences
- Supersedes the brainstorm's "omp and opencode drivers" (`docs/research/harness-os.md`).
- The benchmark gains an opencode arm; the arm set is a later record of this program (amends 0482).
- Amends 0482 ("No API keys": one key, for A7 only) and 0506 (opencode is signed in with that key in the VM for A7; H5 and H6 stay as 0506 has them).
