# 0112 Project rename entry

- Status: decided
- Kind: product
- Phase/slice: 1/1.2
- Date: 2026-10-07

## Question
How does `projects.yaml` hold a project's rename?

## Facts
- 0110 shows the repo name, else the folder name; 0111 has no `project rename`, so a rename is a hand edit.
- Project ids are stable across clones and machines (0107).

## Options
| Option | Pros | Cons |
|---|---|---|
| A `names:` map keyed by project id | Survives moves and re-clones | Needs the id, which `project list` shows |
| A `names:` map keyed by path | Readable | Breaks when the main path moves |

## Recommendation
A `names:` map keyed by id.

## Choice
A `names:` map keyed by project id:

```yaml
names:
  p-1a2b3c4d5e6f7a8b: demo-web
```

## Reason
Not given.

## Consequences
- `projects.yaml` holds `version`, `roots`, `add`, `exclude` and `names` (0109).
