# 0221 Trust prompt and command

- Status: dropped
- Kind: product
- Phase/slice: 2/2.3
- Date: 2026-10-08

## Question
When and how is trust asked, and which command manages it?

## Facts
- Pi's built-in prompt appears at startup when protected resources exist; print, JSON and RPC modes skip protected resources unless `--approve` (`docs/security.md:75-82`).
- Appendix I claims `project trust` for phase 2.

## Options
| Option | Pros | Cons |
|---|---|---|
| A Jarvis prompt at thread start listing what was found or changed; headless runs skip and say so; `jarvis project trust <project> [--revoke]` | Shows what changed | A prompt to build |
| Pi's prompt as it is | Nothing to build | Doesn't show changes |
| Headless runs exit 3 instead of skipping | Explicit | Runs stop when they could proceed |

## Recommendation
As recommended.

## Choice
Now (0307): dropped; no Jarvis trust prompt or `project trust` command.

Originally: A prompt when a thread starts in a project with new or changed gated resources, listing each as new, changed or same, with yes, no and view-diff. Headless runs skip untrusted resources and print a notice. `jarvis project trust <project>` trusts and lists resources with the hash; `--revoke` removes the decision.

```
Project acme-api has resources that run code:
  new      .claude/settings.json  (2 hooks)
  changed  .jarvis/extensions/lint.ts
  same     .mcp.json  (1 server)
Trust acme-api?  [y] yes  [n] no  [v] view diff
```

## Reason
Not given.

## Consequences
- A project-trust decision isn't an approval, so headless runs don't use exit 3 for it.
- Amended by 0307: dropped in the pruning pass.
- Amended by 0391: omp's approvals are the guard; Jarvis's guard retires and the fixture gate ends.
