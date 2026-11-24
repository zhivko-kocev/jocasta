# 0684 Harness contract: omp's own screen, capabilities, no adapter layer

- Status: v1
- Kind: product
- Phase/slice: lightweight program
- Date: 2026-11-24

## Question
How does Jocasta stay ready for omp2 and for omp updates without an adapter layer?

## Facts
- omp's `--mode rpc` has no interactive screen; Jocasta's panes are omp's interactive mode.
- The bridge (0675) sits on omp's extension API (`pi.sendUserMessage`, `ctx.abort`, `ctx.shutdown`, `ctx.getContextUsage`, `ctx.getAsyncJobSnapshot`).
- Pins are re-checked at the seam (0383).

## Options
| Option | Pros | Cons |
|---|---|---|
| An adapter layer with several harness back ends | Swappable harness | Code for harnesses that don't exist; re-implements omp |
| Capabilities reported by the extension; the service degrades by them; omp-specific code in two places | Small; omp2 replaces two files; an omp update that drops a method shows as a degraded pane, not a crash | The degradations must be tested |
| Assume every omp method exists | Least code | A renamed method breaks panes silently |

## Recommendation
Capabilities reported by the extension, no adapter layer.

## Choice
His answer of 2026-11-24: omp keeps its own interactive screen (not `--mode rpc`). At load the extension sends `caps: ["prompt"]` in `hello` when `pi.sendUserMessage` is a function; on `session_start` it sends `{"t":"event","name":"caps","caps":[…]}` with `abort`, `shutdown`, `usage`, `jobs` for each of `ctx.abort`, `ctx.shutdown`, `ctx.getContextUsage`, `ctx.getAsyncJobSnapshot` that is a function. The service keeps the set per peer and degrades:
- no `prompt`: the pane takes no service prompts and one NEEDS YOU event says `omp <version> can't take prompts from Jocasta; check the pin`;
- no `abort` or `shutdown`: wherever a step says abort or shutdown, the service ends the pane's process tree (`proctree.rs`) and respawns with `--restore`;
- no `usage`: the context part of the row is omitted;
- no `jobs`: the pane never sleeps by itself.
`config/bridge-extension.ts` and `pane_launch.rs`'s argv are the only omp-specific code; nothing else in the service names omp, so a later omp2 channel replaces those two.

## Reason
This is the omp2 preparation: no adapter layer.

## Consequences
- A test covers each degradation (a peer whose `caps` event lacks `shutdown`, then a sleep of that pane).
- If omp2 ships a channel better than an extension, only the extension and the argv change; the pipe protocol and service stay.
