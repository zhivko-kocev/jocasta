# 0162 Fake script format

- Status: decided
- Kind: product
- Phase/slice: 1/1.10
- Date: 2026-10-07

## Question
What format does a `JARVIS_FAKE_SCRIPT` file use?

## Facts
- Principle 5: YAML for Jarvis's own files.
- The faux provider takes assistant messages built from text, thinking and tool calls, with simulated usage (`@earendil-works/pi-ai` 1.0.1, `dist/providers/faux.d.ts`).

## Options
| Option | Pros | Cons |
|---|---|---|
| YAML, one entry per model turn | Readable and diffable; principle 5 | A YAML parser in the fake path |
| JSONL of faux messages | No translation | Verbose; tied to pi-ai's shape |

## Recommendation
YAML turns.

## Choice
Now (0376): the format stands, read by the scripted-model test fixture.

Originally: YAML with `version` and a list of `turns`; each turn has `text`, `tool_calls` (each `name` and `args`) or both, and optional `usage` (`input`, `output`, `cache_read`):

```yaml
version: 1
turns:
  - tool_calls:
      - name: bash
        args: { command: "go list ./..." }
  - text: "Two packages: ./cmd/demo and ./internal/retry."
    usage: { input: 1200, output: 40, cache_read: 1000 }
```

## Reason
Not given.

## Consequences
- Each smoke's offline twin ships one script.
- Amended by 0386: the harness is whole omp with config only; this Jarvis harness piece retires in favor of omp's own.
