# 0208 Rule grammar

- Status: decided
- Kind: product
- Phase/slice: 2/2.1
- Date: 2026-10-08

## Question
How is a rule written in `permissions.yaml`?

## Facts
Verified 2026-10-08 (https://code.claude.com/docs/en/permissions): Claude Code writes rules as `Tool(specifier)` strings such as `Bash(git push *)` and `Read(./.env)`, with gitignore-style paths.
- His Claude Code setup has about 100 deny rules (appendix A). Jarvis has deny, ask and allow (0197) and history conditions (0205).

## Options
| Option | Pros | Cons |
|---|---|---|
| Claude Code style strings under `deny`, `ask` and `allow` | His rules port by copy; one-line rules | Claude's tool names map onto Jarvis's tools |
| Capability objects | Explicit fields | Wordier; rules need rewriting |

## Recommendation
Claude Code style strings.

## Choice
Now (0231): the same grammar with Jarvis's and Pi's tool names, such as `bash(git push --force *)` and `read(**/.env*)`.

Originally:
Claude Code style strings under `deny`, `ask` and `allow`; a rule can carry an `after:` history condition (0205):

```yaml
version: 1
deny:
  - Bash(git push --force *)
  - Read(**/.env*)
ask:
  - Bash(git push *)
  - WebFetch
allow:
  - Bash(bun test *)
  - Bash(git commit *):
      after: Bash(bun test *) passed since last Edit
```

## Reason
Not given.

## Consequences
- Claude's tool names (`Bash`, `Read`, `Edit`, `Write`, `WebFetch`, `mcp__…`) map onto Jarvis's and Pi's tools; the map is part of the build.
- The exact `after:` syntax is fixed in the build's schema with the previewed meaning.
- Amended by 0231: rules use Jarvis's and Pi's tool names, not Claude's.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
