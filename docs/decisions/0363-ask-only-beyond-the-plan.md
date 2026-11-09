# 0363 Ask only about what goes beyond the plan

- Status: decided
- Kind: process
- Phase/slice: v1 beta
- Date: 2026-10-09

## Question
During the v1 beta, which visible details still go to him, and what changes in the harness polish list?

## Facts
- 0005 made anything visible or lasting his call, so every builder's undecided text, label and default came to him as tick-lists.
- The v1 beta (2026-10-09) produced dozens of such items; he ticked them all.

## Options
| Option | Pros | Cons |
|---|---|---|
| Keep asking every visible detail | Nothing is chosen without him | Slows the beta; most are within decided intent |
| Ask only what goes outside the plan or enhances it | Faster; his time goes to real choices | Small texts and defaults are chosen by the builders |

## Recommendation
None; his instruction.

## Choice
From now on he is asked only about things that go outside the plan or enhance the current setup. Details that carry out a decided record (texts, labels, small defaults, fixes that restore decided behavior) are chosen by the builders, follow the existing conventions, and are listed as built in the records.

Harness polish, all changed as asked:
- the startup list names Jarvis's extensions instead of `<inline:N>`; now (0378) the list is hidden by the seeded `quietStartup: "header"` and shows with `--verbose`;
- Pi's exit hint "To resume this session: …" is replaced by Jarvis's own way back (`jarvis --thread <id>`), or nothing outside a thread;
- the approval overlay takes number keys as in 0224's preview;
- a headless run that stops on a question or a denied approval prints one stderr line naming what waits;
- `jarvis thread show` shows the stored report, and the thread's state follows it;
- the trust card keeps Pi's "Trust parent folder" option;
- `jarvis project add` on a project already added says so;
- the owner-lock refusal reads "jarvis: thread <title> (<id>) is already open in another Jarvis (pid N)." with exit 2.

## Reason
"we have a good plan ask only for stuff that go outside the plan and enhance the current setup"

## Consequences
- Narrows 0005 for the rest of the v1 beta: features, new dependencies, formats, paths and changes to a decided record still go to him.
- AGENTS.md's "No drift" rules say so.
