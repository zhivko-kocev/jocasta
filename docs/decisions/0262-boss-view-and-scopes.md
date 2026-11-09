# 0262 What the boss sees, and scopes

- Status: decided
- Kind: product
- Phase/slice: 4/4.4, 1.3
- Date: 2026-10-08

## Question
What does the boss see across client projects, and what are scopes?

## Facts
Read 2026-10-08 in firstmate (kunchenguid/firstmate, commit b062eb9, MIT): digests work from state and reports, not conversation history (`.agents/skills/bearings/SKILL.md:55`).
- 0113 made scopes v1, built here with 0092's agenda (levels, kinds, sealing defaults, mapping, override, file, command). Lean Jarvis (0229).

## Options
| Option | Pros | Cons |
|---|---|---|
| Status only, plus one scope label per project | Client specifics stay out of the boss; smallest scopes | No inheritance tree |
| Status only, plus v4's `me → org → client → project` tree | Inheritance | A tree file and command |
| The boss sees everything | — | A cross-client store |

## Recommendation
Status only plus one label.

## Choice
The boss sees states, titles, report summaries and questions, never thread content. A scope is one label per project in `~/.jarvis/agent/projects.yaml`, keyed by project id, used by phase 5's sealing:

```yaml
scopes:
  p-1a2b3c4d5e6f7a8b: client-a
  p-9f8e7d6c5b4a3f2e: me
```

## Reason
Not given.

## Consequences
- Amends 0109 (`projects.yaml` gains `scopes:`) and settles 0092's agenda: no levels, kinds or `scope` command; appendix I drops `scope`.
- Amended by 0392: herdr is the work surface and the Electrobun app becomes the companion; Jarvis's PTY host, thread tabs and in-process boss retire.
