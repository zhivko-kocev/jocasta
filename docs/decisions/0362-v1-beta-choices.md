# 0362 Choices from the v1 beta

- Status: decided
- Kind: product
- Phase/slice: 4/4.4, 4.8, 4.9, 4.12, 7.4
- Date: 2026-10-09

## Question
Which visible choices made while fixing the v1 beta's bugs are accepted, and what changes?

## Facts
Found on 2026-10-09 in the compiled exe with a real Copilot model:
- the boss approved a thread's plan by itself;
- the first hold of Space downloaded the 487 MB dictation model without asking (against 0330);
- Edge keeps audio suspended until a click or key, which stalled speech for the page's life;
- Space did nothing or pressed a focused button when the chat input had no focus;
- a window blur mid-hold ended dictation;
- the HUD showed dollar costs for Copilot, a flat subscription.

## Options
A tick-list of the built choices, then one question each for spend and the window items.

## Recommendation
Accept the built choices; hide spend for subscription providers.

## Choice
- Accepted as built:
  - The boss may pass on an answer to a thread's question only after he has written or spoken to it since the question opened. Otherwise its tool gets "he hasn't answered "<question>" yet: relay the question to him and pass on only his answer."
  - With no speech model, the first hold of Space shows "Dictation's speech model (about 490 MB) downloads once and stays in Jarvis's data folder. Download it?", with download and not now.
  - Speech that arrives before his first click or key waits, and plays on it.
  - The chat input takes focus when the page connects and on each tab change; a Space pressed outside a text field goes to the input.
  - A hold survives a focus loss while the key repeats, and ends 1.5 seconds after the repeats stop.
- HUD spend for subscription providers (Copilot, the Anthropic subscription) is shown as a list-price equivalent, labelled so.
- Only one Frontier window can be open: a second `jarvis` brings the open window forward. His words: "only one frontier window can be opened".
- Quit closes the window too.
- The boss's thinking shows dimmed on its own line and is kept on reload.
- A needs-you toast closes once its question is answered anywhere.
- His note on the relay rule: "in the future boss can also talk to the threads"; asked what he meant, he said "that will be discussed after v1".

## Reason
Ticked, plus his words above.

## Consequences
- Amends 0274 (spend for subscription providers) and 0332 (a needs-you toast stays until clicked).
- After v1: what more the boss may say to threads.
- Differs from the recommendation on spend.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
- Amended by 0428: the one window sits on the second display when present.
