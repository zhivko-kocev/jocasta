# 0714 `show` spec

- Status: v1 (built 2026-11-27)
- Kind: product, prompts
- Phase/slice: ultron 0.3 iteration, M2 R8
- Date: 2026-11-27
- Amends: 0679 (the spec is checked whole by one reader, strict keys, limits, reference sections)

## Question
The boss's `show` took any keys, any label length and any diff, and the manual named the section types without their fields, so a first `show` often failed or was guessed at. What does a spec allow, who reads it, and what does the boss have to send for a thread's changes?

## Facts
- `show` parsed each section with the brief model's own serde types: unknown keys were dropped silently, `layer` and `order` were accepted from the boss, an empty `sections` was stored as an empty window.
- The window draws a node label in a box of about 80 characters, an edge label of about 24 and a facts label column of `fit-content(16em)` (0713, 0721); a diff over 2 000 lines is what `thread_brief` already cuts at (`DIFF_LINES`).
- `thread_brief` reads a thread's branch against trunk with git (`numstat`, the commits' files, the diff) and builds the map and the diff from it.
- The sections `checks`, `works`, `system`, `cards` and `verdict` carry exit codes, proofs, anchors or his own words that Jocasta ran or checked (0706, 0707, 0710, 0711, 0712).
- `jocasta brief open` (0716) must refuse exactly what `show` refuses.

## Options
| Option | Pros | Cons |
|---|---|---|
| One reader `brief/spec.rs` for the tool and the command: strict keys, limits, map checked like a flow, reference sections filled by code; the manual lists every field | A refusal names the key or the limit; the boss sends 20 tokens for a thread's diff; one place to change | A second set of section types beside the model |
| `deny_unknown_fields` on the model only | Smallest | The stored brief's own fields (`layer`, `order`, a card's proof) become the boss's to send |
| Keep it lenient | Nothing to build | Silent drops; guessing |

## Recommendation
One reader with strict keys, the limits and reference sections.

## Choice
His answers of 2026-11-26, the approved plan and his 2026-11-27 amendment:
- `src/frontier/brief/spec.rs` reads every spec, for the `show` tool and `jocasta brief open`, before anything is stored. Its refusals start `show: `:
  - `sections is empty; give at least one section.`; `title is required.`; `sections is required: an array of sections, each with a \`type\`.`
  - an unknown key, named: at the top `unknown field \`<key>\`; a spec has title and sections.`, in a section or a row serde's `section <n> (<type>) isn't well formed: unknown field \`<key>\`, expected …`.
  - a node label over 80 characters, an edge label over 24, a fact label over 40 (characters, not bytes): `section <n> (flow): node \`<id>\` has a label longer than 80 characters`, `… the edge <a> → <b> has a label longer than 24 characters`, `section <n> (facts): the fact label \`<label>\` is longer than 40 characters`.
  - more than 2 000 diff lines given in all: `the diffs have <n> lines; at most 2000 are drawn.`
  - a diff line's `kind` is `+`, `-`, ` ` or the alias `space` (stored as ` `); anything else: `a line's kind is \`<k>\`; it is +, -, or a space (\`space\`)`.
  - a map's item ids are unique and each link joins two of them, like a flow's nodes and edges: `the map has two items with the id \`<id>\``, `the link <a> – <b> names \`<id>\`, which is no item of the map`. Each flow is laid out by `layout::lay_out_flow` while it is read (its errors as before, prefixed with the section).
- A spec may hold `text`, `facts`, `flow`, `map`, `diff` and `list`. `checks`, `works`, `system`, `cards` and `verdict` stay the review pages' and are filled by code only: they say what Jocasta ran or checked, and a spec could fake that.
- Reference sections: `{"type": "diff", "thread": "<name>"}` and `{"type": "map", "thread": "<name>"}` (a `heading` allowed) are filled by the service from that thread's branch against trunk, as its brief draws them (`thread_brief::reference`; the diff cut at 2 000 lines, the map's links capped as there). The thread is named as `thread_status` names it (pane id, title or branch); an unknown one: `show: section <n> (diff): no thread <name>.` Giving both forms, or neither: `give files or a thread, not both` / `give files, or a thread to fill it from` (`groups` for a map).
- `config/boss-manual.md` lists every section type with its fields and limits, and the reference form, so the first `show` needs no `--help`; the tool's description says the same in short.

## Reason
A refusal that names what to fix makes the boss's second try its last; a reference section keeps a thread's diff out of the boss's context entirely.

## Consequences
- New `brief/spec.rs` (`read`, `Spec`, `Part`, `Reference`, `show_brief`, the limit constants) with `spec_tests.rs`; `tools.rs`'s `show` uses it; `SHOW_SECTIONS` moves there as `SECTIONS`.
- The model's leaf types (`Fact`, `FlowEdge`, `MapGroup`, `MapItem`, `MapLink`, `DiffFile`, `DiffHunk`, `DiffLine`, `ListRow`) refuse unknown fields; stored briefs only ever hold their own fields, so loading is unchanged.
- `thread_brief::reference`; its map takes its heading.
- The tool spec fixture and the boss manual change; the manual grows by eight lines.
