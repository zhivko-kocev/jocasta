# 0710 Review: system view

- Status: v1 (built 2026-11-27)
- Kind: product, prompts
- Phase/slice: ultron 0.3 iteration, M2 R5
- Date: 2026-11-26
- Amends: 0678 (a review page shows what the change does to the system, before its cards), 0721 (the windows draw the `system` section)

## Question
A review page lists findings line by line; nothing on it says what the change does to the system as a whole: which parts it adds, which it removes, what it brings and what it takes away. How does a review show that, without the picture being the model's word?

## Facts
- `layout::lay_out` already places a flow's nodes by layer and order, and the brief windows draw a flow on a canvas with the first-open entrance (0721).
- `review_notes::reads` checks a quote at a line of a committed file; `steps.rs` runs a `{run, expect}` proof where a review's steps run (0706, 0707).
- A pull request carries its base and head SHAs; a thread's review runs on `<trunk>...<branch>`, whose base is the merge base.
- `iced::Animation` animates a value between two states with a custom easing and can be sent back mid-way.

## Options
| Option | Pros | Cons |
|---|---|---|
| A tool that hands in a Before and an After graph, each part anchored to code, plus Brings / Takes away rows with proofs or anchors; one layout over both graphs | The picture is checked against the code like a note; one layout means nothing jumps when he switches | A reviewer must anchor every part |
| A free-form diagram in the page's text | Nothing to build | Unchecked; another drawing to trust |
| No system view | Nothing to build | The page stays a list of lines |

## Recommendation
The anchored Before / After graphs with Brings / Takes away.

## Choice
His answers of 2026-11-26 and the approved plan:
- New thread tool `review_system`, for the pull-request reviewer and a thread's reviewer: `{before: {nodes, edges}, after: {nodes, edges}, brings: [row], takes: [row]}` or `{no_system_change: true}`. A node is `{id, label, anchor: {file, line, quote}}`, an edge `{from, to, label?, anchor}`, a row `{text, proof: {run, expect}}` or `{text, anchor}`. At most 12 nodes per graph.
- Every anchor is checked with the note quote check: the Before graph's (nodes and edges) and the Takes away rows' against the base commit (a pull request's base SHA; a thread's merge base of its trunk and HEAD), the After graph's and the Brings rows' against the head. A row's proof runs once like a `review_try` step (same place, same guard, same judgement) and must reproduce.
- A second call replaces the first. A review that ended (0706) refuses it, as `review_note` does.
- A review that hands in neither form is told once, at its turn end, exactly: `review_system is missing: hand in the before and after graphs, or no_system_change`; its next turn end goes on as before whatever it did.
- New section kind `system`: one layout (`layout::lay_out`) over the union of both graphs; a node or edge in both is `kept`, only in After `added`, only in Before `removed`. The window shows After by default with a Before / After switch, then the Brings and Takes away lists, a proven row with its `✓ proven` disclosure, an anchored one with its `file:line`. `{no_system_change: true}` shows `No change to the system.`
- Built details: the page's order is facts, works, system, then the cards; a node shows its label and, under it, its mark and `file:line`; added parts are green, removed red, kept ones neutral. A graph's ids are unique, an edge joins two nodes of its own graph, both graphs empty is refused (`hand in no_system_change instead`). Refusals name the part: `review_system: after node <id>: <file>:<line> at <sha> doesn't read that. Lines a–b are: …`.
- Motion (0721's curve and the 220 ms token): switching fades and scales added nodes in from 0.96 (removed ones fade out) over 220 ms, edges fade with their nodes, and a switch pressed mid-way turns back from where it is. Reduced motion keeps the fade only.

## Reason
What the page says about the system is anchored to the code at the right commit, so he can trust the picture as much as a proven finding, and one layout lets him see the difference by switching rather than by comparing two drawings.

## Consequences
- `model.rs`: `Section::System {nodes, edges, brings, takes, no_change}`, `SystemNode`, `SystemEdge`, `SystemRow`, `Anchor`, `Mark`; `layout::lay_out` lays out a system's union.
- New `server/review_system.rs` (`REVIEW_SYSTEM`, `MISSING`, `review_system`); `review.json` gains `system` and `system_asked`; `works::prove` names its tool.
- `review_flow.rs`' turn end asks once; `pr_review.rs` and `thread_brief.rs` put the section after works.
- `tool_specs.rs` (and its fixture) describe the tool, a long-running one; `config/review.yml` allows it unasked; `config/pr-review.md` and `review_prompt.rs` ask for it.
- `view/`: the switch, the union drawn with marks, the rows.
