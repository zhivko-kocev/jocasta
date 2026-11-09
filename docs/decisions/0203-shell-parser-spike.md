# 0203 Shell parser spike rule

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
Which parser splits compound shell commands for the guard?

## Facts
- Lesson L26's hand-written tokenizer passed its 52-case corpus on this machine (`t5-safety/LESSON-26-shell-analysis.md:459-463`), with known gaps (here-docs, loop bodies).
- opencode v1.18.22 ships `web-tree-sitter` 0.25.10 with `tree-sitter-bash` 0.25.0 and `tree-sitter-powershell` 0.25.10, loaded lazily as WASM (`packages/opencode/package.json:143-149`).
- Codex keeps a strict parser that may prove a command safe and a loose one used only to find danger (lesson L26).
- Neither tree-sitter package is in Jarvis's dependencies; adding one needs his approval at build time.
- A decision that needs a measurement is recorded as a rule (0086).

## Options
| Option | Pros | Cons |
|---|---|---|
| A spike rule measured on 2.2's corpus at build time | The result picks the parser | A spike to run |
| Tree-sitter without a spike | A real grammar, PowerShell included | Load time and size unmeasured |
| The hand parser without a spike | No dependency | Known gaps |

## Recommendation
The spike rule.

## Choice
Built with 2.1, measured on 2.2's corpus on Windows:
- the hand parser: corpus pass rate and size in lines;
- tree-sitter with the bash and PowerShell grammars (WASM): pass rate, cold load time and exe size increase.

Rule: if tree-sitter passes every case, loads in under 150 ms and adds under 5 MB, use tree-sitter; else if the hand parser passes every case, use it; else use the one with more passes, and its failing cases become corpus cases that ask. A command neither parser can parse is treated as ask, or deny when its raw text matches a deny pattern. A result the rule doesn't settle reopens the decision.

## Reason
Not given.

## Consequences
- Installing tree-sitter for the spike asks him first (no installs without asking).
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
