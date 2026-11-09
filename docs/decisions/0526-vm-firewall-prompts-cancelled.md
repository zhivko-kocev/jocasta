# 0526 Every Firewall prompt in the VM is cancelled and noted

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
Cancelling the Node.js prompt (0525) brought up a second Firewall prompt queued behind it, for "dlna.test", with the same full-screen overlay. More may be queued, and the full run's task tests could raise new ones. What should the agent do with these?

## Facts
Verified 2026-11-02 in the VM, in tester's session.
- The Node.js prompt went at the Cancel click (12:29Z); a prompt for "dlna.test" (Publisher Unknown) stood in its place at once, and the overlay (`Shell_SystemDim`) stayed.
- `dlna.test` is a Go test binary, most likely from rclone's tests in the pilot or task validation [INFERENCE].
- Cancel keeps a program blocked for inbound connections, which is Windows' default while a prompt stands. Loopback traffic isn't affected, and Go writes each test binary to a fresh temp folder.

## Options
| Option | Pros | Cons |
|---|---|---|
| Cancel every one | Frees the desktop for the runs; keeps the default block | A block rule per program may be written |
| Ask him each time | Each prompt is his | Each one blocks the desktop, H2's conditions and the click checks until he answers |
| Allow every one | Frees the desktop | Opens each test binary to inbound connections, which no test needs |

## Recommendation
Cancel every one.

## Choice
Every queued Firewall prompt in the VM, and any new one during the runs, is cancelled the same way and noted in that run's notes (the program it names and the time).

## Reason
"Cancel every one".

## Consequences
- Amends 0525: a prompt that appears again no longer goes to him.
