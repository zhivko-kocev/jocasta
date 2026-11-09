# 0518 A herdr server that dies under `jocasta`'s terminal is started again and its client attached again

- Status: decided
- Kind: product
- Phase/slice: program
- Date: 2026-11-02

## Question
H3's first fault killed herdr's server while `jocasta` ran herdr's client in its terminal. Jocasta took the client's exit for him closing, asked "Stop 8 threads awaiting you? Their sessions are saved." and nothing came back until that was answered. H3 is pre-registered on 0431's restart. What should a dead herdr server do while `jocasta` runs in the terminal?

## Facts
Verified 2026-11-02 in the VM's benchmark sandbox, on 0517's build.
- H3 killed session `bench-jc`'s server at 03:52:57Z. Its client in the sandbox terminal logged "connection was lost" and exited at 03:52:59Z (`herdr-client.log`).
- `openSurface` (`src/shared/cli.ts:229-233`) takes any exit of herdr's client that the companion didn't cause (0491) for him closing and runs `close("client")`, so the companion asked the close question (8 threads awaiting him). 0434: "0431's herdr restart never runs during a stop."
- Unanswered, herdr stayed down for 14 minutes, and H3 stopped after 1 of its 60 runs ("herdr session bench-jc lists no panes").
- With Keep pressed in the sandbox (a diagnostic, 04:06:43Z), the service started herdr again within 5 s ("herdr's server for session bench-jc had stopped; Jocasta started it again") and relaunched every pane herdr restored as a shell. Killed again with no client attached (09:43:16Z), herdr was started again within 0.7 s and listed the same pane ids 2.3 s after the kill.
- 0482's H3: "recovered when herdr lists the same pane ids again (the companion restarts it, 0431)".

## Options
| Option | Pros | Cons |
|---|---|---|
| Heal: restart and re-attach | The surface heals itself, as 0431 says; H3 runs as designed, on the setup he uses | Amends 0434 for this one case; a new build before H3 |
| Keep it; H3 records it | No product change; H3 at once | No recovery time for a herdr crash in his setup; the thesis reports a crash that closes Jocasta |
| Measure 0431 without the terminal | Measures 0431 as decided; H3 at once | Not the setup he works in; a crash there still asks to close |

## Recommendation
Heal: restart and re-attach.

## Choice
Heal: when herdr's client exits while its server is gone and the companion didn't stop it, `jocasta` doesn't close Jocasta. It waits for the companion to start herdr again (0431) and attaches herdr's client again in the same terminal. Closing the terminal tab, quitting herdr's client while its server runs, and quitting the companion still stop everything behind the close question (0434). With no companion answering, or herdr not back within 30 seconds, `jocasta` closes as before.

## Reason
"Heal: restart and re-attach".

## Consequences
- Amends 0434: a dead herdr server is no longer a close.
- H3's herdr fault measures the restart and the re-attach on the setup `jocasta` makes.
