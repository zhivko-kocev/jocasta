# 0525 The agent cancels the VM's Firewall prompt for Node.js

- Status: decided
- Kind: process
- Phase/slice: program
- Date: 2026-11-02

## Question
The VM's Windows Firewall prompt for "Node.js JavaScript Runtime" has stood unanswered since H2 on 0517's build. It now blocks the benchmarks' window conditions and every real mouse click. What should the agent do with it?

## Facts
Verified 2026-11-02 in the VM, in tester's session.
- The prompt (`Shell_SystemDialog` in `PickerHost`, at 283..741 × 151..568) lays a disabled, topmost overlay over the whole screen (`Shell_SystemDim`, 0,0..1024,768). Every real mouse click in the session lands on the overlay or on the prompt, so no click on Jocasta's windows can be tested.
- A live check's click landed on the prompt at 11:44Z; since then it holds the foreground. `SetForegroundWindow`, `SwitchToThisWindow` and a synthesized Alt+Tab didn't move it. The companion went on counting itself focused and opened no toast window.
- H2's "behind the terminal" condition (0520) needs the terminal brought to the front. The 0517 release's background pass that raised no toasts, and the attempt where the terminal never came to the front, both ran with the prompt on screen.
- The prompt says Node.js is blocked on public and private networks; Cancel keeps it blocked.

## Options
| Option | Pros | Cons |
|---|---|---|
| Cancel it | Node stays blocked, as now; frees the desktop, so H2 runs all three conditions and real clicks can be tested | Windows may write a block rule; it may come back if a node process listens beyond loopback |
| Allow it | Frees the desktop | Windows writes allow rules that open node.exe to inbound connections on both networks, which nothing needs |
| Keep it unanswered | Nothing changes in the VM | H2's "behind the terminal" row is dropped; the real clicks on Jocasta's windows move to his PC |

## Recommendation
Cancel it.

## Choice
The agent cancels the prompt through its Cancel button, after noting which program it names.

## Reason
"Cancel it".

## Consequences
- Amends 0516 for this one step outside the benchmarks' own steps.
- H2 runs its three conditions as 0520 says, and the real mouse clicks on 0519's windows are tested in the VM.
- A prompt that appears again goes to him.
- Amended by 0526: every Firewall prompt in the VM, queued or new, is cancelled the same way and noted; none goes to him.
