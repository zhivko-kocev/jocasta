# 0694 `jocasta setup --yes`

- Status: v1 (built 2026-11-27)
- Kind: product
- Phase/slice: ultron 0.3 iteration, J6
- Date: 2026-11-26
- Amends: 0466 (setup writes after `y`, or without the question under `--yes`)

## Question
ultron's installer runs `jocasta setup` on a fresh machine, where nobody sits at the terminal to type `y`. How does an installer write setup's changes?

## Facts
- `jocasta setup` prints every pending change (each migration's diff), asks `Write these changes? [y/N]` once and writes all of them only after `y` (0466); then it asks for the calendar's address and the mail and Jira secrets when their sources are configured and no secret is stored (0430, 0680).
- A piped `y` works but hides intent, and the secret questions after it would read the next lines of the pipe.

## Options
| Option | Pros | Cons |
|---|---|---|
| A `--yes` flag: the same list, written without the question | One obvious word for scripts; he still sees what changed | One more flag |
| Pipe `y` | Nothing to build | Fragile; the secret questions read the pipe too |

## Recommendation
`--yes`.

## Choice
His answers of 2026-11-26 and the approved plan:
- `jocasta setup --yes` (`-y`) prints the same list of changes, then writes every pending change without asking, with the same exit codes (1 when a migration fails).
- It asks nothing afterwards either: the calendar's address and the mail and Jira secrets are skipped (a person types them later with `jocasta setup`).
- `--yes` and `--refresh` don't go together (a usage error, exit 2).

## Reason
An installer can set Jocasta up in one step and its log still shows what was written.

## Consequences
- `Route::Setup` gains `yes`; `run_setup` takes `Asking::Ask` or `Asking::Never`.
- ultron's installer runs `jocasta setup --yes` when `~/.jocasta/agent/setup` holds no markers.
