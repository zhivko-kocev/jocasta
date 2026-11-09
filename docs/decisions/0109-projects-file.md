# 0109 The projects file

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
Which file lists watched roots, and where do `project add` and `project remove` persist?

## Facts
- The file lives in `~/.jarvis/agent` (0094).
- Principle 3: one owner per setting. Principle 5: YAML for Jarvis's own config.

## Options
| Option | Pros | Cons |
|---|---|---|
| One `projects.yaml` with `roots`, `add` and `exclude` | Every choice he made is in one hand-editable file; the catalog can be rebuilt from it | The commands write YAML and must keep comments |
| `roots.yaml` with roots only; adds and removes in `catalog.db` | The YAML is never machine-written | Wiping the data dir forgets adds and removes |

## Recommendation
`projects.yaml` with roots, adds and exclusions.

## Choice
Now (0262): `projects.yaml` also holds `scopes:`, one label per project id.

Originally:
`~/.jarvis/agent/projects.yaml`:

```yaml
version: 1
roots:
  - path: D:/
    depth: 1
    ignore: ["*.wt", "node_modules", "$RECYCLE.BIN"]
add:
  - D:/notes
exclude:
  - D:/old-fork
```

## Reason
Not given.

## Consequences
- `project add` and `project remove` write `add` and `exclude`, keeping comments.
- The catalog is a cache of this file plus the scan.
- Amended by 0262.
